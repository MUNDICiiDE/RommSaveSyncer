use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use romm_sync_planner::{
    ActivityStatus, AdapterSnapshot, AdapterStatus, BaselineSnapshot, BaselineState,
    CompatibilityStatus, Fingerprint, GameKey, HashAlgorithm, IdentityStatus, Platform,
    RemoteSaveSnapshot, RemoteSaveState, SaveModel, SlotId,
};
use serde::Deserialize;

use crate::{CoreError, io_error, validate_relative};

pub const DISCOVERY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryManifest {
    pub schema_version: u32,
    pub items: Vec<ManifestItem>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestItem {
    pub platform: String,
    pub rom_id: String,
    pub slot: String,
    pub adapter: ManifestAdapter,
    pub identity: String,
    pub activity: String,
    pub root: PathBuf,
    pub files: Vec<String>,
    pub remote: ManifestRemote,
    pub baseline: ManifestBaseline,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestAdapter {
    pub id: String,
    pub revision: String,
    pub status: String,
    pub save_model: String,
    pub compatibility: String,
    pub can_upload: bool,
    pub can_download: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManifestRemote {
    Absent,
    Present {
        content_sha256: String,
        revision: String,
        slot: String,
        extension: String,
        size: u64,
    },
    Deleted {
        last_revision: Option<String>,
    },
    Unavailable,
    Malformed,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManifestBaseline {
    None,
    Known {
        local_content_sha256: String,
        remote_content_sha256: String,
        remote_revision: String,
    },
    Invalid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverySpec {
    pub key: GameKey,
    pub adapter: AdapterSnapshot,
    pub identity: IdentityStatus,
    pub activity: ActivityStatus,
    pub root: PathBuf,
    pub files: Vec<String>,
    pub remote: RemoteSaveState,
    pub baseline: BaselineState,
}

/// Loads and validates a versioned discovery manifest.
///
/// # Errors
///
/// Returns an error for unreadable JSON, an unsupported schema, unknown enum
/// values, or unsafe save paths.
pub fn load_discovery_manifest(path: &Path) -> Result<Vec<DiscoverySpec>, CoreError> {
    let bytes = fs::read(path).map_err(|error| io_error("read", path, error))?;
    let manifest: DiscoveryManifest = serde_json::from_slice(&bytes)?;
    manifest.into_specs()
}

impl DiscoveryManifest {
    /// Converts the external manifest into strict discovery inputs.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported schema, duplicate game keys, unknown
    /// values, or unsafe save paths.
    pub fn into_specs(self) -> Result<Vec<DiscoverySpec>, CoreError> {
        if self.schema_version != DISCOVERY_SCHEMA_VERSION {
            return Err(CoreError::InvalidManifest(format!(
                "unsupported schema version {}; expected {DISCOVERY_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        if self.items.is_empty() {
            return Err(CoreError::InvalidManifest(
                "at least one discovery item is required".to_owned(),
            ));
        }
        let mut keys = BTreeSet::new();
        let mut specs = Vec::with_capacity(self.items.len());
        for item in self.items {
            let spec = item.into_spec()?;
            for relative in &spec.files {
                validate_relative(relative)?;
            }
            let key = (
                spec.key.platform.slug(),
                spec.key.rom_id.clone(),
                spec.key.slot.0.clone(),
            );
            if !keys.insert(key) {
                return Err(CoreError::InvalidManifest(format!(
                    "duplicate game key: {}/{}/{}",
                    spec.key.platform.slug(),
                    spec.key.rom_id,
                    spec.key.slot.0
                )));
            }
            specs.push(spec);
        }
        Ok(specs)
    }
}

impl ManifestItem {
    fn into_spec(self) -> Result<DiscoverySpec, CoreError> {
        let platform = parse_platform(&self.platform)?;
        let slot = SlotId(self.slot);
        let adapter = AdapterSnapshot {
            adapter_id: self.adapter.id,
            adapter_revision: self.adapter.revision,
            platform,
            status: parse_adapter_status(&self.adapter.status)?,
            save_model: parse_save_model(&self.adapter.save_model)?,
            compatibility: parse_compatibility(&self.adapter.compatibility)?,
            can_upload: self.adapter.can_upload,
            can_download: self.adapter.can_download,
            expected_slot: slot.clone(),
        };

        Ok(DiscoverySpec {
            key: GameKey {
                platform,
                rom_id: self.rom_id,
                slot,
            },
            adapter,
            identity: parse_identity(&self.identity)?,
            activity: parse_activity(&self.activity)?,
            root: self.root,
            files: self.files,
            remote: self.remote.into_state(),
            baseline: self.baseline.into_state(),
        })
    }
}

impl ManifestRemote {
    fn into_state(self) -> RemoteSaveState {
        match self {
            Self::Absent => RemoteSaveState::Absent,
            Self::Present {
                content_sha256,
                revision,
                slot,
                extension,
                size,
            } => RemoteSaveState::Present(RemoteSaveSnapshot {
                content: sha256_fingerprint(content_sha256),
                revision,
                slot: SlotId(slot),
                extension,
                size,
            }),
            Self::Deleted { last_revision } => RemoteSaveState::Deleted { last_revision },
            Self::Unavailable => RemoteSaveState::Unavailable,
            Self::Malformed => RemoteSaveState::Malformed,
        }
    }
}

impl ManifestBaseline {
    fn into_state(self) -> BaselineState {
        match self {
            Self::None => BaselineState::None,
            Self::Known {
                local_content_sha256,
                remote_content_sha256,
                remote_revision,
            } => BaselineState::Known(BaselineSnapshot {
                local_content: sha256_fingerprint(local_content_sha256),
                remote_content: sha256_fingerprint(remote_content_sha256),
                remote_revision,
            }),
            Self::Invalid => BaselineState::Invalid,
        }
    }
}

fn sha256_fingerprint(digest: String) -> Fingerprint {
    Fingerprint::new(HashAlgorithm::Sha256, digest)
}

/// Parses a `RomM` platform slug for one of the twelve target platforms.
///
/// # Errors
///
/// Returns an error for an unknown slug.
pub fn parse_platform(value: &str) -> Result<Platform, CoreError> {
    match value {
        "nds" => Ok(Platform::NintendoDs),
        "gba" => Ok(Platform::GameBoyAdvance),
        "ngc" => Ok(Platform::GameCube),
        "nes" => Ok(Platform::NintendoEntertainmentSystem),
        "snes" => Ok(Platform::SuperNintendo),
        "n64" => Ok(Platform::Nintendo64),
        "wii" => Ok(Platform::Wii),
        "3ds" => Ok(Platform::Nintendo3Ds),
        "psx" => Ok(Platform::PlayStation),
        "ps2" => Ok(Platform::PlayStation2),
        "psp" => Ok(Platform::PlayStationPortable),
        "switch" => Ok(Platform::NintendoSwitch),
        _ => invalid_value("platform", value),
    }
}

fn parse_adapter_status(value: &str) -> Result<AdapterStatus, CoreError> {
    match value {
        "validated" => Ok(AdapterStatus::Validated),
        "pending_validation" => Ok(AdapterStatus::PendingValidation),
        "unsupported" => Ok(AdapterStatus::Unsupported),
        "temporarily_unsafe" => Ok(AdapterStatus::TemporarilyUnsafe),
        _ => invalid_value("adapter status", value),
    }
}

fn parse_save_model(value: &str) -> Result<SaveModel, CoreError> {
    match value {
        "single_file" => Ok(SaveModel::SingleFile),
        "multi_file" => Ok(SaveModel::MultiFile),
        "game_specific_memory_card" => Ok(SaveModel::GameSpecificMemoryCard),
        "directory_tree" => Ok(SaveModel::DirectoryTree),
        "shared_memory_card" => Ok(SaveModel::SharedMemoryCard),
        "unknown" => Ok(SaveModel::Unknown),
        _ => invalid_value("save model", value),
    }
}

fn parse_compatibility(value: &str) -> Result<CompatibilityStatus, CoreError> {
    match value {
        "compatible" => Ok(CompatibilityStatus::Compatible),
        "incompatible" => Ok(CompatibilityStatus::Incompatible),
        "unknown" => Ok(CompatibilityStatus::Unknown),
        _ => invalid_value("compatibility", value),
    }
}

fn parse_identity(value: &str) -> Result<IdentityStatus, CoreError> {
    match value {
        "verified" => Ok(IdentityStatus::Verified),
        "ambiguous" => Ok(IdentityStatus::Ambiguous),
        "missing" => Ok(IdentityStatus::Missing),
        _ => invalid_value("identity", value),
    }
}

fn parse_activity(value: &str) -> Result<ActivityStatus, CoreError> {
    match value {
        "inactive" => Ok(ActivityStatus::Inactive),
        "active" => Ok(ActivityStatus::Active),
        "unknown" => Ok(ActivityStatus::Unknown),
        _ => invalid_value("activity", value),
    }
}

fn invalid_value<T>(field: &str, value: &str) -> Result<T, CoreError> {
    Err(CoreError::InvalidManifest(format!(
        "unknown {field} value {value:?}"
    )))
}
