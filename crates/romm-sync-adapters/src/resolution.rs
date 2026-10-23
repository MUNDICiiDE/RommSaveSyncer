use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use romm_sync_core::{
    CoreError, DiscoverySpec, ExecutionHook, FileMapping, SavePayload, discover, sha256_bytes,
    validate_relative,
};
use romm_sync_planner::{
    ActivityStatus, AdapterSnapshot, BaselineState, CompatibilityStatus, GameKey, IdentityStatus,
    LocalSaveState, ReconciliationInput, RemoteSaveState, SaveModel,
};
use serde::Serialize;

use crate::{
    AdapterDefinition, ReadinessReason, battery::SizeRule, gci::GameId, gci::validate_gci_owner,
    readiness::save_model_name,
};

/// Adapter-specific format rules for one owned save member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MemberFormat {
    Battery(SizeRule),
    Gci(GameId),
}

impl MemberFormat {
    fn validate(&self, bytes: &[u8]) -> Result<(), ReadinessReason> {
        match self {
            Self::Battery(rule) => {
                if rule.accepts(bytes.len() as u64) {
                    Ok(())
                } else {
                    Err(ReadinessReason::BatterySizeUnrecognized)
                }
            }
            Self::Gci(game_id) => validate_gci_owner(bytes, game_id),
        }
    }

    fn accepts_remote_size(&self, size: u64) -> bool {
        match self {
            Self::Battery(rule) => rule.accepts(size),
            Self::Gci(_) => crate::gci::plausible_gci_length(size),
        }
    }
}

/// How the planner-facing adapter descriptor is produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotMode {
    /// Use the reviewed readiness stage. Only target-validated adapters may mutate.
    Production,
    /// Disposable qualification: a fixture-validated adapter is presented as
    /// validated under a distinct adapter identity. Callers must restrict this
    /// to copied saves in a disposable workspace.
    Qualification,
}

/// A read-only, adapter-specific interpretation of one game's save location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterResolution {
    pub definition: &'static AdapterDefinition,
    pub save_model: SaveModel,
    /// Directory passed to M1B discovery and execution as the save root.
    pub root: PathBuf,
    /// Adapter-owned members relative to `root`.
    pub files: Vec<String>,
    /// Local-to-RomM member mapping for M1B.
    pub mappings: Vec<FileMapping>,
    /// Filename `RomM` should store for uploads.
    pub upload_file_name: String,
    /// Emulator metadata recorded on `RomM` uploads.
    pub romm_emulator: &'static str,
    /// Structural reasons that make this save non-executable.
    pub blocking: Vec<ReadinessReason>,
    pub compatibility: CompatibilityStatus,
    /// Forces a malformed local state when ownership cannot be proven.
    pub local_malformed: bool,
    pub diagnostics: Vec<String>,
    pub(crate) format: MemberFormat,
    pub(crate) remote_extensions: &'static [&'static str],
}

/// Normalized evidence collected outside the adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationContext {
    pub key: GameKey,
    pub identity: IdentityStatus,
    pub activity: ActivityStatus,
    pub remote: RemoteSaveState,
    pub baseline: BaselineState,
    pub stability_delay: Duration,
    pub mode: SnapshotMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedSave {
    pub input: ReconciliationInput,
    pub reasons: Vec<ReadinessReason>,
    pub diagnostics: Vec<String>,
}

impl AdapterResolution {
    /// Planner descriptor for this save. Structural blocks lower compatibility.
    #[must_use]
    pub fn adapter_snapshot(
        &self,
        key: &GameKey,
        compatibility: CompatibilityStatus,
        mode: SnapshotMode,
    ) -> AdapterSnapshot {
        let (adapter_id, status) =
            if mode == SnapshotMode::Qualification && self.definition.qualifiable() {
                (
                    format!("{}+qualification", self.definition.configuration_id),
                    romm_sync_planner::AdapterStatus::Validated,
                )
            } else {
                (
                    self.definition.configuration_id.to_owned(),
                    self.definition.status(),
                )
            };
        AdapterSnapshot {
            adapter_id,
            adapter_revision: self.definition.revision.to_owned(),
            platform: self.definition.platform,
            status,
            save_model: self.save_model,
            compatibility,
            can_upload: self.definition.can_upload,
            can_download: self.definition.can_download,
            expected_slot: key.slot.clone(),
        }
    }

    /// Validates one adapter-owned member's bytes.
    ///
    /// # Errors
    ///
    /// Returns the stable reason the bytes cannot belong to this save.
    pub fn validate_member(&self, bytes: &[u8]) -> Result<(), ReadinessReason> {
        self.format.validate(bytes)
    }

    /// Checks the current `RomM` head against this adapter's accepted formats.
    ///
    /// # Errors
    ///
    /// Returns a reason when the remote head must block instead of being
    /// silently skipped in favor of an older compatible version.
    pub fn assess_remote(&self, remote: &RemoteSaveState) -> Result<(), ReadinessReason> {
        let RemoteSaveState::Present(snapshot) = remote else {
            return Ok(());
        };
        if !self.accepts_remote_extension(&snapshot.extension) {
            return Err(ReadinessReason::RemoteFormatUnsupported);
        }
        if !self.format.accepts_remote_size(snapshot.size) {
            return Err(ReadinessReason::RemoteSizeInvalid);
        }
        Ok(())
    }

    /// Whether a remote filename or bare extension uses an accepted format.
    #[must_use]
    pub fn accepts_remote_extension(&self, value: &str) -> bool {
        let extension = Path::new(value)
            .extension()
            .map_or(value, |extension| extension.to_str().unwrap_or_default());
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        self.remote_extensions.contains(&extension.as_str())
    }

    /// Observes, hashes, and format-validates the resolved save group.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths or unreadable files.
    pub fn observe(&self, context: &ObservationContext) -> Result<ObservedSave, CoreError> {
        let mut reasons = self.blocking.clone();
        let mut diagnostics = self.diagnostics.clone();
        let mut compatibility = self.compatibility;
        if let Err(reason) = self.assess_remote(&context.remote) {
            reasons.push(reason);
            compatibility = CompatibilityStatus::Incompatible;
        }
        let spec = DiscoverySpec {
            key: context.key.clone(),
            adapter: self.adapter_snapshot(&context.key, compatibility, context.mode),
            identity: context.identity,
            activity: context.activity,
            root: self.root.clone(),
            files: self.files.clone(),
            remote: context.remote.clone(),
            baseline: context.baseline.clone(),
        };
        let mut discovered = discover(&[spec], context.stability_delay)?
            .pop()
            .ok_or_else(|| CoreError::InvalidManifest("discovery returned no item".to_owned()))?;
        diagnostics.append(&mut discovered.diagnostics);
        let mut input = discovered.input;

        // Unattributable files block writes even when the owned save is absent.
        if self.local_malformed {
            input.local = LocalSaveState::Malformed;
        }
        if let (LocalSaveState::Present(group), romm_sync_planner::RootStatus::Available(root)) =
            (&input.local, &input.root)
            && self.save_model == SaveModel::SingleFile
        {
            let resolved = Path::new(&root.resolved_path);
            for file in &group.files {
                let path = resolved.join(&file.relative_path);
                let bytes = fs::read(&path).map_err(|source| CoreError::Io {
                    operation: "read",
                    path: path.clone(),
                    source,
                })?;
                if sha256_bytes(&bytes) != file.integrity || bytes.len() as u64 != file.size {
                    diagnostics.push("files_changed_during_validation".to_owned());
                    input.local = LocalSaveState::Incomplete;
                    break;
                }
                if let Err(reason) = self.validate_member(&bytes) {
                    reasons.push(reason);
                    input.local = LocalSaveState::Malformed;
                    break;
                }
            }
        }
        reasons.sort();
        reasons.dedup();
        Ok(ObservedSave {
            input,
            reasons,
            diagnostics,
        })
    }

    /// Serializable read-only summary for CLI inspection.
    #[must_use]
    pub fn summary(&self, observed: &ObservedSave) -> InspectionReport {
        let (local_state, members) = match &observed.input.local {
            LocalSaveState::Absent => ("absent", Vec::new()),
            LocalSaveState::Incomplete => ("incomplete", Vec::new()),
            LocalSaveState::Malformed => ("malformed", Vec::new()),
            LocalSaveState::Present(group) => (
                "present",
                group
                    .files
                    .iter()
                    .map(|file| InspectedMember {
                        relative_path: file.relative_path.clone(),
                        size: file.size,
                        sha256: file.integrity.digest.clone(),
                    })
                    .collect(),
            ),
        };
        InspectionReport {
            schema_version: 1,
            read_only: true,
            configuration_id: self.definition.configuration_id.to_owned(),
            stage: self.definition.stage,
            save_model: save_model_name(self.save_model).to_owned(),
            compatibility: match observed.input.adapter.compatibility {
                CompatibilityStatus::Compatible => "compatible",
                CompatibilityStatus::Incompatible => "incompatible",
                CompatibilityStatus::Unknown => "unknown",
            }
            .to_owned(),
            root_available: matches!(
                observed.input.root,
                romm_sync_planner::RootStatus::Available(_)
            ),
            local_state: local_state.to_owned(),
            members,
            upload_file_name: self.upload_file_name.clone(),
            reasons: observed.reasons.clone(),
            diagnostics: observed.diagnostics.clone(),
        }
    }
}

impl ExecutionHook for AdapterResolution {
    fn before_replace(&self, _index: usize, _destination: &Path) -> Result<(), CoreError> {
        Ok(())
    }

    fn validate_payload(&self, _key: &GameKey, payload: &SavePayload) -> Result<(), String> {
        if payload.members.len() != self.mappings.len() {
            return Err(ReadinessReason::PayloadMemberUnexpected.as_str().to_owned());
        }
        for member in &payload.members {
            if !self
                .mappings
                .iter()
                .any(|mapping| mapping.remote_member == member.name)
            {
                return Err(ReadinessReason::PayloadMemberUnexpected.as_str().to_owned());
            }
            if self.save_model != SaveModel::SingleFile {
                return Err(ReadinessReason::MultiFileTransportUnsupported
                    .as_str()
                    .to_owned());
            }
            self.validate_member(&member.bytes)
                .map_err(|reason| reason.as_str().to_owned())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InspectionReport {
    pub schema_version: u32,
    pub read_only: bool,
    pub configuration_id: String,
    pub stage: crate::ReadinessStage,
    pub save_model: String,
    pub compatibility: String,
    pub root_available: bool,
    pub local_state: String,
    pub members: Vec<InspectedMember>,
    pub upload_file_name: String,
    pub reasons: Vec<ReadinessReason>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InspectedMember {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

/// Accepts a single safe filename component.
pub(crate) fn safe_file_name(value: &str) -> bool {
    validate_relative(value).is_ok()
        && !value.contains(['/', '\\'])
        && value != "."
        && !value.starts_with('.')
}
