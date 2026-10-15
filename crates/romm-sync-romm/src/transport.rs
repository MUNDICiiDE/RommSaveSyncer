use std::collections::BTreeMap;

use romm_sync_core::{CoreError, PayloadMember, RemoteTransport, SavePayload};
use romm_sync_planner::{
    ExpectedRemoteState, Fingerprint, GameKey, HashAlgorithm, RemoteSaveSnapshot, RemoteSaveState,
};
use sha2::{Digest, Sha256};

use crate::{RomMClient, UploadOutcome, VerifiedDownload};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteMapping {
    pub rom_id: i64,
    /// Emulator folder identity preserved on `RomM` uploads.
    pub emulator: Option<String>,
    /// Stable logical member name used by M1B's local-to-remote mapping.
    pub remote_member: String,
    /// Filename `RomM` should store, including adapter-selected extension.
    pub upload_file_name: String,
}

pub struct RomMTransport {
    client: RomMClient,
    session_id: i64,
    mappings: BTreeMap<GameKey, RemoteMapping>,
    cache: BTreeMap<GameKey, CachedSave>,
}

#[derive(Clone)]
struct CachedSave {
    download: VerifiedDownload,
    snapshot: RemoteSaveSnapshot,
}

impl RomMTransport {
    #[must_use]
    pub fn new(
        client: RomMClient,
        session_id: i64,
        mappings: BTreeMap<GameKey, RemoteMapping>,
    ) -> Self {
        Self {
            client,
            session_id,
            mappings,
            cache: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn client(&self) -> &RomMClient {
        &self.client
    }

    fn mapping(&self, key: &GameKey) -> Result<&RemoteMapping, CoreError> {
        self.mappings.get(key).ok_or_else(|| {
            CoreError::Remote(format!(
                "no RomM mapping for {}:{}:{}",
                key.platform.slug(),
                key.rom_id,
                key.slot.0
            ))
        })
    }

    fn refresh(&mut self, key: &GameKey) -> Result<Option<CachedSave>, CoreError> {
        let mapping = self.mapping(key)?.clone();
        let Some(save) = self
            .client
            .latest_save(mapping.rom_id, &key.slot.0)
            .map_err(remote_error)?
        else {
            self.cache.remove(key);
            return Ok(None);
        };
        if let Some(cached) = self.cache.get(key)
            && cached.download.save.id == save.id
            && cached.download.save.content_hash == save.content_hash
        {
            return Ok(Some(cached.clone()));
        }
        let verified = self
            .client
            .download_unconfirmed(save, self.session_id)
            .map_err(remote_error)?;
        let snapshot = snapshot_from_download(&verified, &key.slot.0);
        let cached = CachedSave {
            download: verified,
            snapshot,
        };
        self.cache.insert(key.clone(), cached.clone());
        Ok(Some(cached))
    }
}

impl RemoteTransport for RomMTransport {
    fn observe(&mut self, key: &GameKey) -> Result<RemoteSaveState, CoreError> {
        self.refresh(key).map(|remote| {
            remote.map_or(RemoteSaveState::Absent, |cached| {
                RemoteSaveState::Present(cached.snapshot)
            })
        })
    }

    fn download(
        &mut self,
        key: &GameKey,
        expected: &RemoteSaveSnapshot,
    ) -> Result<SavePayload, CoreError> {
        let mapping = self.mapping(key)?.clone();
        let current = self.refresh(key)?.ok_or_else(|| {
            CoreError::Remote("RomM save disappeared before download staging".to_owned())
        })?;
        if current.snapshot != *expected {
            return Err(CoreError::Remote(
                "RomM save revision changed before download staging".to_owned(),
            ));
        }
        Ok(SavePayload {
            members: vec![PayloadMember {
                name: mapping.remote_member,
                bytes: current.download.bytes,
            }],
            content: current.snapshot.content,
        })
    }

    fn upload(
        &mut self,
        key: &GameKey,
        payload: &SavePayload,
        expected: &ExpectedRemoteState,
    ) -> Result<RemoteSaveSnapshot, CoreError> {
        let mapping = self.mapping(key)?.clone();
        let observed = self.observe(key)?;
        if !matches_expected(&observed, expected) {
            return Err(CoreError::Remote(
                "RomM save revision changed immediately before upload".to_owned(),
            ));
        }
        let [member] = payload.members.as_slice() else {
            return Err(CoreError::Remote(
                "RomM 5.4 transfer boundary accepts one staged file per operation".to_owned(),
            ));
        };
        if member.name != mapping.remote_member || sha256(&member.bytes) != payload.content {
            return Err(CoreError::Remote(
                "upload staging bytes or member identity changed".to_owned(),
            ));
        }
        let outcome = self
            .client
            .upload_autosave(
                mapping.rom_id,
                self.session_id,
                &key.slot.0,
                &mapping.upload_file_name,
                mapping.emulator.as_deref(),
                member.bytes.clone(),
            )
            .map_err(remote_error)?;
        let save = match outcome {
            UploadOutcome::Stored(save) | UploadOutcome::AlreadyPresent(save) => save,
        };
        let snapshot = RemoteSaveSnapshot {
            content: payload.content.clone(),
            revision: save.revision(),
            slot: key.slot.clone(),
            extension: extension(&mapping.upload_file_name),
            size: member.bytes.len() as u64,
        };
        let download = VerifiedDownload {
            md5: save.content_hash.clone().unwrap_or_default(),
            save,
            bytes: member.bytes.clone(),
        };
        self.cache.insert(
            key.clone(),
            CachedSave {
                download,
                snapshot: snapshot.clone(),
            },
        );
        Ok(snapshot)
    }

    fn confirm_download(
        &mut self,
        key: &GameKey,
        committed: &RemoteSaveSnapshot,
    ) -> Result<(), CoreError> {
        let cached = self.cache.get(key).ok_or_else(|| {
            CoreError::Remote("download confirmation has no verified staged payload".to_owned())
        })?;
        if cached.snapshot != *committed {
            return Err(CoreError::Remote(
                "download confirmation revision differs from committed payload".to_owned(),
            ));
        }
        self.client
            .confirm_download(cached.download.save.id, &cached.download.md5)
            .map_err(remote_error)
    }
}

fn snapshot_from_download(download: &VerifiedDownload, slot: &str) -> RemoteSaveSnapshot {
    RemoteSaveSnapshot {
        content: sha256(&download.bytes),
        revision: download.save.revision(),
        slot: slot.into(),
        extension: extension(&download.save.file_name),
        size: download.bytes.len() as u64,
    }
}

fn matches_expected(observed: &RemoteSaveState, expected: &ExpectedRemoteState) -> bool {
    match (observed, expected) {
        (RemoteSaveState::Absent, ExpectedRemoteState::Absent) => true,
        (RemoteSaveState::Present(remote), ExpectedRemoteState::Present { content, revision }) => {
            remote.content == *content && remote.revision == *revision
        }
        _ => false,
    }
}

fn sha256(bytes: &[u8]) -> Fingerprint {
    Fingerprint::new(
        HashAlgorithm::Sha256,
        crate::client::hex_lower(&Sha256::digest(bytes)),
    )
}

fn extension(file_name: &str) -> String {
    std::path::Path::new(file_name)
        .extension()
        .map_or_else(String::new, |value| {
            value.to_string_lossy().to_ascii_lowercase()
        })
}

#[allow(clippy::needless_pass_by_value)]
fn remote_error(error: crate::RomMError) -> CoreError {
    CoreError::Remote(error.to_string())
}
