use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use fs2::FileExt;
use romm_sync_planner::{
    ActivityStatus, AdapterSnapshot, ExpectedLocalState, ExpectedRemoteState, Fingerprint, GameKey,
    LocalSaveState, MutationPreconditions, PlanAction, PlannedItem, RemoteSaveSnapshot,
    RemoteSaveState, RootSnapshot, RootStatus,
};
use serde::{Deserialize, Serialize};

use crate::{
    BackupManager, BackupPolicy, BackupRecord, CoreError, JournalState, StateStore, io_error,
    key_slug, observe_local_group, observe_root, read_all, replace_file, safe_segment,
    validate_relative, write_all_synced,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMapping {
    pub local_relative_path: String,
    pub remote_member: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionRequest {
    pub planned: PlannedItem,
    pub mappings: Vec<FileMapping>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadMember {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavePayload {
    pub members: Vec<PayloadMember>,
    pub content: Fingerprint,
}

pub trait RemoteTransport {
    /// Re-reads the current remote revision and content identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote state cannot be observed reliably.
    fn observe(&mut self, key: &GameKey) -> Result<RemoteSaveState, CoreError>;

    /// Downloads the exact revision named by `expected`.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision is unavailable, transport fails, or
    /// the payload cannot be represented safely.
    fn download(
        &mut self,
        key: &GameKey,
        expected: &RemoteSaveSnapshot,
    ) -> Result<SavePayload, CoreError>;

    /// Uploads a payload only if the remote still matches `expected`.
    ///
    /// # Errors
    ///
    /// Returns an error when the conditional write is rejected, transport
    /// fails, or the resulting remote identity cannot be verified.
    fn upload(
        &mut self,
        key: &GameKey,
        payload: &SavePayload,
        expected: &ExpectedRemoteState,
    ) -> Result<RemoteSaveSnapshot, CoreError>;

    /// Acknowledges a download after its payload has been atomically committed
    /// and read back from the live save path.
    ///
    /// Transports without an acknowledgement protocol may use the default.
    /// `RomM` uses this hook for `POST /api/saves/{id}/downloaded`; doing it in
    /// `download` would advance the device baseline before the local write is
    /// known to be durable.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote acknowledgement is not durably
    /// accepted. The committed local save is preserved and no sync baseline is
    /// recorded, allowing a later idempotent retry.
    fn confirm_download(
        &mut self,
        _key: &GameKey,
        _committed: &RemoteSaveSnapshot,
    ) -> Result<(), CoreError> {
        Ok(())
    }
}

pub trait ActivityProbe {
    /// Reports whether the relevant emulator or game can be proven inactive.
    ///
    /// # Errors
    ///
    /// Returns an error when activity detection itself cannot run.
    fn observe(&self, key: &GameKey) -> Result<ActivityStatus, CoreError>;
}

pub trait SpaceProbe {
    /// Returns usable bytes on the filesystem containing `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when free space cannot be determined.
    fn available_bytes(&self, path: &Path) -> Result<u64, CoreError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemSpaceProbe;

impl SpaceProbe for SystemSpaceProbe {
    fn available_bytes(&self, path: &Path) -> Result<u64, CoreError> {
        fs2::available_space(path).map_err(|error| io_error("query free space", path, error))
    }
}

pub trait ExecutionHook {
    /// Runs immediately before each staged member replaces its destination.
    ///
    /// # Errors
    ///
    /// Returns an error to simulate or report a replacement failure.
    fn before_replace(&self, index: usize, destination: &Path) -> Result<(), CoreError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopExecutionHook;

impl ExecutionHook for NoopExecutionHook {
    fn before_replace(&self, _index: usize, _destination: &Path) -> Result<(), CoreError> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ExecutorConfig {
    pub lock_root: PathBuf,
    pub backup_root: PathBuf,
    pub backup_policy: BackupPolicy,
    pub stability_delay: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionCode {
    NonMutatingPlan,
    MissingPreconditions,
    LockBusy,
    InvalidMapping,
    AdapterChanged,
    RootChanged,
    MountChanged,
    LocalChanged,
    RemoteChanged,
    ActivityActive,
    ActivityUnknown,
    InsufficientSpace,
    PayloadInvalid,
    CommitFailed,
    RecoveryDeferred,
}

impl RejectionCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NonMutatingPlan => "non_mutating_plan",
            Self::MissingPreconditions => "missing_preconditions",
            Self::LockBusy => "lock_busy",
            Self::InvalidMapping => "invalid_mapping",
            Self::AdapterChanged => "adapter_changed",
            Self::RootChanged => "root_changed",
            Self::MountChanged => "mount_changed",
            Self::LocalChanged => "local_changed",
            Self::RemoteChanged => "remote_changed",
            Self::ActivityActive => "activity_active",
            Self::ActivityUnknown => "activity_unknown",
            Self::InsufficientSpace => "insufficient_space",
            Self::PayloadInvalid => "payload_invalid",
            Self::CommitFailed => "commit_failed",
            Self::RecoveryDeferred => "recovery_deferred",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ExecutionOutcome {
    Uploaded {
        journal_id: i64,
        bytes: u64,
        remote_revision: String,
    },
    Downloaded {
        journal_id: i64,
        bytes: u64,
        backup_path: Option<PathBuf>,
    },
    Rejected {
        code: RejectionCode,
        detail: String,
    },
}

pub struct GuardedExecutor {
    config: ExecutorConfig,
    backups: BackupManager,
}

struct GameLock {
    file: File,
}

#[derive(Serialize, Deserialize)]
struct TransactionManifest {
    relative_files: Vec<String>,
}

impl Drop for GameLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

impl GuardedExecutor {
    #[must_use]
    pub fn new(config: ExecutorConfig) -> Self {
        let backups = BackupManager::new(config.backup_root.clone(), config.backup_policy);
        Self { config, backups }
    }

    #[must_use]
    pub fn lock_path(&self, key: &GameKey) -> PathBuf {
        self.config
            .lock_root
            .join(format!("{}.lock", safe_segment(&key_slug(key))))
    }

    #[allow(clippy::too_many_arguments)]
    /// Executes one previously planned mutation after reacquiring every guard.
    ///
    /// # Errors
    ///
    /// Returns an error for storage, database, transport, or recovery failures.
    /// Stale or unsafe preconditions are returned as typed rejected outcomes.
    pub fn execute(
        &self,
        store: &StateStore,
        request: &ExecutionRequest,
        current_adapter: &AdapterSnapshot,
        remote: &mut impl RemoteTransport,
        activity: &impl ActivityProbe,
        space: &impl SpaceProbe,
        hook: &impl ExecutionHook,
    ) -> Result<ExecutionOutcome, CoreError> {
        if !request.planned.action.is_mutation() {
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::NonMutatingPlan,
                "only upload and download plans are executable",
                None,
            );
        }
        let Some(preconditions) = request.planned.preconditions.as_ref() else {
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::MissingPreconditions,
                "mutation plan has no stale-observation guards",
                None,
            );
        };
        if let Err((code, detail)) = validate_mappings(request, preconditions) {
            return self.reject(store, &request.planned.key, code, &detail, None);
        }
        let Some(_lock) = self.try_lock(&request.planned.key)? else {
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::LockBusy,
                "another execution holds the per-game lock",
                None,
            );
        };

        if let Err((code, detail)) = self.revalidate(
            request,
            preconditions,
            current_adapter,
            remote,
            activity,
            space,
        )? {
            return self.reject(store, &request.planned.key, code, &detail, None);
        }

        let journal_id = store.begin_execution(
            &request.planned.key,
            request.planned.action,
            &preconditions.root,
        )?;
        match request.planned.action {
            PlanAction::Upload => self.execute_upload(
                store,
                journal_id,
                request,
                preconditions,
                current_adapter,
                remote,
                activity,
                space,
            ),
            PlanAction::Download => self.execute_download(
                store,
                journal_id,
                request,
                preconditions,
                current_adapter,
                remote,
                activity,
                space,
                hook,
            ),
            _ => unreachable!("non-mutating actions are rejected before journaling"),
        }
    }

    /// Recovers all non-terminal journal entries when activity and mount guards permit it.
    ///
    /// # Errors
    ///
    /// Returns an error when journal data is inconsistent or a verified rollback
    /// cannot be completed.
    pub fn recover_interrupted(
        &self,
        store: &StateStore,
        activity: &impl ActivityProbe,
    ) -> Result<Vec<ExecutionOutcome>, CoreError> {
        let mut outcomes = Vec::new();
        for entry in store.unfinished_journals()? {
            let activity_state = activity.observe(&entry.key)?;
            if activity_state != ActivityStatus::Inactive {
                outcomes.push(ExecutionOutcome::Rejected {
                    code: RejectionCode::RecoveryDeferred,
                    detail: format!("activity is {activity_state:?}"),
                });
                continue;
            }
            let observed_root = observe_root(&entry.root_path);
            let RootStatus::Available(root) = observed_root else {
                outcomes.push(ExecutionOutcome::Rejected {
                    code: RejectionCode::RecoveryDeferred,
                    detail: "journal root is unavailable".to_owned(),
                });
                continue;
            };
            if root.mount_identity != entry.mount_identity {
                outcomes.push(ExecutionOutcome::Rejected {
                    code: RejectionCode::RecoveryDeferred,
                    detail: "journal mount identity changed".to_owned(),
                });
                continue;
            }

            if entry.state == JournalState::Replacing && entry.action == "download" {
                if let Some(backup_path) = &entry.backup_path {
                    let record = self.backups.load_record(backup_path)?;
                    self.backups.restore(&record, &entry.root_path)?;
                } else if let Some(stage_path) = &entry.stage_path {
                    let targets = read_transaction_manifest(stage_path)?;
                    self.backups
                        .remove_created_files(&entry.root_path, &targets)?;
                } else {
                    return Err(CoreError::DatabaseIntegrity(format!(
                        "replacing journal {} has no recovery metadata",
                        entry.journal_id
                    )));
                }
            }
            let recovery_reason =
                if entry.state == JournalState::Replacing && entry.action == "upload" {
                    "interrupted_upload_requires_remote_revalidation"
                } else {
                    "interrupted_execution"
                };
            if let Some(stage_path) = &entry.stage_path {
                remove_directory_if_exists(stage_path)?;
            }
            store.update_journal(entry.journal_id, JournalState::Recovered, None, None, None)?;
            store.record_history(
                &entry.key,
                &entry.action,
                "recovered",
                Some(recovery_reason),
                0,
            )?;
        }
        Ok(outcomes)
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_upload(
        &self,
        store: &StateStore,
        journal_id: i64,
        request: &ExecutionRequest,
        preconditions: &MutationPreconditions,
        current_adapter: &AdapterSnapshot,
        remote: &mut impl RemoteTransport,
        activity: &impl ActivityProbe,
        space: &impl SpaceProbe,
    ) -> Result<ExecutionOutcome, CoreError> {
        self.wait_for_stability();
        if let Err((code, detail)) = self.revalidate(
            request,
            preconditions,
            current_adapter,
            remote,
            activity,
            space,
        )? {
            return self.reject(store, &request.planned.key, code, &detail, Some(journal_id));
        }
        let payload = match build_upload_payload(request, preconditions) {
            Ok(payload) => payload,
            Err(error) => {
                return self.reject(
                    store,
                    &request.planned.key,
                    RejectionCode::LocalChanged,
                    &error.to_string(),
                    Some(journal_id),
                );
            }
        };
        let bytes = payload_size(&payload)?;
        if let Err((code, detail)) = self.revalidate(
            request,
            preconditions,
            current_adapter,
            remote,
            activity,
            space,
        )? {
            return self.reject(store, &request.planned.key, code, &detail, Some(journal_id));
        }
        store.update_journal(journal_id, JournalState::Replacing, None, None, None)?;
        let receipt = match remote.upload(
            &request.planned.key,
            &payload,
            &preconditions.expected_remote,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                store.update_journal(
                    journal_id,
                    JournalState::Failed,
                    None,
                    None,
                    Some(&error.to_string()),
                )?;
                return Err(error);
            }
        };
        if receipt.content != payload.content || receipt.slot != request.planned.key.slot {
            store.update_journal(
                journal_id,
                JournalState::Failed,
                None,
                None,
                Some("remote receipt did not match uploaded payload"),
            )?;
            return Err(CoreError::Remote(
                "remote receipt did not match uploaded payload".to_owned(),
            ));
        }
        store.upsert_baseline(
            &request.planned.key,
            &payload.content.digest,
            &receipt.content.digest,
            &receipt.revision,
        )?;
        store.update_journal(journal_id, JournalState::Committed, None, None, None)?;
        store.record_history(
            &request.planned.key,
            "upload",
            "verified",
            Some(request.planned.reason.as_str()),
            bytes,
        )?;
        Ok(ExecutionOutcome::Uploaded {
            journal_id,
            bytes,
            remote_revision: receipt.revision,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn execute_download(
        &self,
        store: &StateStore,
        journal_id: i64,
        request: &ExecutionRequest,
        preconditions: &MutationPreconditions,
        current_adapter: &AdapterSnapshot,
        remote: &mut impl RemoteTransport,
        activity: &impl ActivityProbe,
        space: &impl SpaceProbe,
        hook: &impl ExecutionHook,
    ) -> Result<ExecutionOutcome, CoreError> {
        let ExpectedRemoteState::Present { content, revision } = &preconditions.expected_remote
        else {
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::PayloadInvalid,
                "download requires a present remote precondition",
                Some(journal_id),
            );
        };
        let expected_remote = match &request.planned.action {
            PlanAction::Download => RemoteSaveSnapshot {
                content: content.clone(),
                revision: revision.clone(),
                slot: request.planned.key.slot.clone(),
                extension: String::new(),
                size: preconditions.minimum_free_bytes,
            },
            _ => unreachable!(),
        };
        let payload = remote.download(&request.planned.key, &expected_remote)?;
        let live_root = Path::new(&preconditions.root.resolved_path);
        let stage_path = live_root
            .join(".romm-autosync-staging")
            .join(format!("{journal_id:020}"));
        if stage_path.exists() {
            return Err(CoreError::Rejected(format!(
                "staging directory already exists: {}",
                stage_path.display()
            )));
        }
        fs::create_dir_all(&stage_path)
            .map_err(|error| io_error("create staging directory", &stage_path, error))?;
        let targets: Vec<_> = request
            .mappings
            .iter()
            .map(|mapping| mapping.local_relative_path.clone())
            .collect();
        write_transaction_manifest(&stage_path, &targets)?;
        if let Err((code, detail)) = stage_payload(&stage_path, request, &payload) {
            remove_directory_if_exists(&stage_path)?;
            return self.reject(store, &request.planned.key, code, &detail, Some(journal_id));
        }
        store.update_journal(
            journal_id,
            JournalState::Staged,
            None,
            Some(&stage_path),
            None,
        )?;
        let staged_root = RootSnapshot {
            logical_path: stage_path.to_string_lossy().into_owned(),
            resolved_path: stage_path
                .canonicalize()
                .map_err(|error| io_error("canonicalize staging", &stage_path, error))?
                .to_string_lossy()
                .into_owned(),
            mount_identity: preconditions.root.mount_identity.clone(),
        };
        let staged = observe_local_group(&staged_root, &targets)?;
        let LocalSaveState::Present(staged_group) = staged else {
            remove_directory_if_exists(&stage_path)?;
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::PayloadInvalid,
                "staged payload is incomplete",
                Some(journal_id),
            );
        };
        if staged_group.content != *content || payload.content != *content {
            remove_directory_if_exists(&stage_path)?;
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::PayloadInvalid,
                "staged payload hash does not match the planned remote content",
                Some(journal_id),
            );
        }

        if let Err((code, detail)) = self.revalidate(
            request,
            preconditions,
            current_adapter,
            remote,
            activity,
            space,
        )? {
            remove_directory_if_exists(&stage_path)?;
            return self.reject(store, &request.planned.key, code, &detail, Some(journal_id));
        }

        let backup = match &preconditions.expected_local {
            ExpectedLocalState::Absent => None,
            ExpectedLocalState::Present { files, .. } => self.backups.create_routine_backup(
                journal_id,
                &request.planned.key,
                live_root,
                files,
            )?,
        };
        if let Err((code, detail)) = self.revalidate(
            request,
            preconditions,
            current_adapter,
            remote,
            activity,
            space,
        )? {
            remove_directory_if_exists(&stage_path)?;
            return self.reject(store, &request.planned.key, code, &detail, Some(journal_id));
        }
        store.update_journal(
            journal_id,
            JournalState::BackedUp,
            backup.as_ref().map(|record| record.directory.as_path()),
            Some(&stage_path),
            None,
        )?;
        store.update_journal(
            journal_id,
            JournalState::Replacing,
            None,
            Some(&stage_path),
            None,
        )?;

        if let Err(error) = commit_staged(&stage_path, live_root, request, hook) {
            self.rollback(live_root, &targets, backup.as_ref())?;
            remove_directory_if_exists(&stage_path)?;
            store.update_journal(
                journal_id,
                JournalState::RolledBack,
                None,
                None,
                Some(&error.to_string()),
            )?;
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::CommitFailed,
                &error.to_string(),
                None,
            );
        }

        let committed = observe_local_group(&preconditions.root, &targets)?;
        if committed != LocalSaveState::Present(staged_group.clone()) {
            self.rollback(live_root, &targets, backup.as_ref())?;
            remove_directory_if_exists(&stage_path)?;
            store.update_journal(
                journal_id,
                JournalState::RolledBack,
                None,
                None,
                Some("post-commit verification failed"),
            )?;
            return self.reject(
                store,
                &request.planned.key,
                RejectionCode::CommitFailed,
                "post-commit verification failed",
                None,
            );
        }

        let bytes = staged_group.total_size;
        if let Err(error) = remote.confirm_download(&request.planned.key, &expected_remote) {
            remove_directory_if_exists(&stage_path)?;
            store.update_journal(
                journal_id,
                JournalState::Failed,
                None,
                None,
                Some(&error.to_string()),
            )?;
            store.record_history(
                &request.planned.key,
                "download_confirmation",
                "failed",
                Some("local_commit_verified_remote_confirmation_pending"),
                bytes,
            )?;
            return Err(error);
        }
        store.upsert_baseline(
            &request.planned.key,
            &staged_group.content.digest,
            &content.digest,
            revision,
        )?;
        store.update_journal(journal_id, JournalState::Committed, None, None, None)?;
        store.record_history(
            &request.planned.key,
            "download",
            "verified",
            Some(request.planned.reason.as_str()),
            bytes,
        )?;
        remove_directory_if_exists(&stage_path)?;
        Ok(ExecutionOutcome::Downloaded {
            journal_id,
            bytes,
            backup_path: backup.map(|record| record.directory),
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::unused_self)]
    fn revalidate(
        &self,
        request: &ExecutionRequest,
        preconditions: &MutationPreconditions,
        current_adapter: &AdapterSnapshot,
        remote: &mut impl RemoteTransport,
        activity: &impl ActivityProbe,
        space: &impl SpaceProbe,
    ) -> Result<Result<(), (RejectionCode, String)>, CoreError> {
        if current_adapter != &preconditions.expected_adapter {
            return Ok(Err((
                RejectionCode::AdapterChanged,
                "adapter predicates or revision changed".to_owned(),
            )));
        }
        let observed_root = observe_root(Path::new(&preconditions.root.logical_path));
        let RootStatus::Available(root) = observed_root else {
            return Ok(Err((
                RejectionCode::RootChanged,
                "save root is missing or unavailable".to_owned(),
            )));
        };
        if root.resolved_path != preconditions.root.resolved_path {
            return Ok(Err((
                RejectionCode::RootChanged,
                "resolved save root changed".to_owned(),
            )));
        }
        if root.mount_identity != preconditions.root.mount_identity {
            return Ok(Err((
                RejectionCode::MountChanged,
                "save root mount identity changed".to_owned(),
            )));
        }
        let local_files = expected_local_paths(preconditions, request);
        let observed_local = observe_local_group(&root, &local_files)?;
        if !local_matches(&preconditions.expected_local, &observed_local) {
            return Ok(Err((
                RejectionCode::LocalChanged,
                "local save group no longer matches the plan".to_owned(),
            )));
        }
        let observed_remote = remote.observe(&request.planned.key)?;
        if !remote_matches(&preconditions.expected_remote, &observed_remote) {
            return Ok(Err((
                RejectionCode::RemoteChanged,
                "remote revision or content changed".to_owned(),
            )));
        }
        match activity.observe(&request.planned.key)? {
            ActivityStatus::Inactive => {}
            ActivityStatus::Active => {
                return Ok(Err((
                    RejectionCode::ActivityActive,
                    "game or emulator is active".to_owned(),
                )));
            }
            ActivityStatus::Unknown => {
                return Ok(Err((
                    RejectionCode::ActivityUnknown,
                    "activity could not be proven inactive".to_owned(),
                )));
            }
        }
        let available = space.available_bytes(Path::new(&root.resolved_path))?;
        if available < preconditions.minimum_free_bytes {
            return Ok(Err((
                RejectionCode::InsufficientSpace,
                format!(
                    "{available} bytes available; {} required",
                    preconditions.minimum_free_bytes
                ),
            )));
        }
        Ok(Ok(()))
    }

    fn rollback(
        &self,
        live_root: &Path,
        targets: &[String],
        backup: Option<&BackupRecord>,
    ) -> Result<(), CoreError> {
        if let Some(record) = backup {
            self.backups.restore(record, live_root)
        } else {
            self.backups.remove_created_files(live_root, targets)
        }
    }

    fn try_lock(&self, key: &GameKey) -> Result<Option<GameLock>, CoreError> {
        fs::create_dir_all(&self.config.lock_root)
            .map_err(|error| io_error("create lock directory", &self.config.lock_root, error))?;
        let lock_path = self.lock_path(key);
        let file = match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(error) if is_lock_contention(&error) => return Ok(None),
            Err(error) => return Err(io_error("open lock", &lock_path, error)),
        };
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(GameLock { file })),
            Err(error) if is_lock_contention(&error) => Ok(None),
            Err(error) => Err(io_error("lock", lock_path, error)),
        }
    }

    fn wait_for_stability(&self) {
        if !self.config.stability_delay.is_zero() {
            thread::sleep(self.config.stability_delay);
        }
    }

    #[allow(clippy::unused_self)]
    fn reject(
        &self,
        store: &StateStore,
        key: &GameKey,
        code: RejectionCode,
        detail: &str,
        journal_id: Option<i64>,
    ) -> Result<ExecutionOutcome, CoreError> {
        if let Some(journal_id) = journal_id {
            store.update_journal(journal_id, JournalState::Rejected, None, None, Some(detail))?;
        }
        store.record_history(key, "execute", "rejected", Some(code.as_str()), 0)?;
        Ok(ExecutionOutcome::Rejected {
            code,
            detail: detail.to_owned(),
        })
    }
}

fn is_lock_contention(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }

    #[cfg(windows)]
    {
        // Windows can report sharing/lock violations while opening the file,
        // before `try_lock_exclusive` has a chance to return `WouldBlock`.
        matches!(error.raw_os_error(), Some(32 | 33))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn validate_mappings(
    request: &ExecutionRequest,
    preconditions: &MutationPreconditions,
) -> Result<(), (RejectionCode, String)> {
    if request.mappings.is_empty() {
        return Err((
            RejectionCode::InvalidMapping,
            "at least one file mapping is required".to_owned(),
        ));
    }
    let mut local = BTreeSet::new();
    let mut remote = BTreeSet::new();
    for mapping in &request.mappings {
        if validate_relative(&mapping.local_relative_path).is_err()
            || validate_relative(&mapping.remote_member).is_err()
            || !local.insert(mapping.local_relative_path.clone())
            || !remote.insert(mapping.remote_member.clone())
        {
            return Err((
                RejectionCode::InvalidMapping,
                "file mappings must be unique safe relative paths".to_owned(),
            ));
        }
    }
    if let ExpectedLocalState::Present { files, .. } = &preconditions.expected_local {
        let expected: BTreeSet<_> = files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect();
        if expected != local {
            return Err((
                RejectionCode::InvalidMapping,
                "mapping does not cover the planned local save group".to_owned(),
            ));
        }
    }
    Ok(())
}

fn expected_local_paths(
    preconditions: &MutationPreconditions,
    request: &ExecutionRequest,
) -> Vec<String> {
    match &preconditions.expected_local {
        ExpectedLocalState::Absent => request
            .mappings
            .iter()
            .map(|mapping| mapping.local_relative_path.clone())
            .collect(),
        ExpectedLocalState::Present { files, .. } => files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect(),
    }
}

fn local_matches(expected: &ExpectedLocalState, observed: &LocalSaveState) -> bool {
    match (expected, observed) {
        (ExpectedLocalState::Absent, LocalSaveState::Absent) => true,
        (
            ExpectedLocalState::Present {
                content,
                integrity,
                files,
            },
            LocalSaveState::Present(group),
        ) => {
            content == &group.content
                && integrity == &group.integrity
                && files == &group.normalized().files
        }
        _ => false,
    }
}

fn remote_matches(expected: &ExpectedRemoteState, observed: &RemoteSaveState) -> bool {
    match (expected, observed) {
        (ExpectedRemoteState::Absent, RemoteSaveState::Absent) => true,
        (ExpectedRemoteState::Present { content, revision }, RemoteSaveState::Present(remote)) => {
            content == &remote.content && revision == &remote.revision
        }
        _ => false,
    }
}

fn build_upload_payload(
    request: &ExecutionRequest,
    preconditions: &MutationPreconditions,
) -> Result<SavePayload, CoreError> {
    let ExpectedLocalState::Present { content, files, .. } = &preconditions.expected_local else {
        return Err(CoreError::Rejected(
            "upload requires a present local save".to_owned(),
        ));
    };
    let root = Path::new(&preconditions.root.resolved_path);
    let mut mappings = request.mappings.clone();
    mappings.sort_by(|left, right| left.local_relative_path.cmp(&right.local_relative_path));
    let expected_files: BTreeMap<_, _> = files
        .iter()
        .map(|file| (file.relative_path.as_str(), file))
        .collect();
    let members = mappings
        .into_iter()
        .map(|mapping| {
            let expected = expected_files
                .get(mapping.local_relative_path.as_str())
                .ok_or_else(|| {
                    CoreError::Rejected(format!(
                        "upload mapping has no planned local member: {}",
                        mapping.local_relative_path
                    ))
                })?;
            let bytes = read_all(&root.join(&mapping.local_relative_path))?;
            if bytes.len() as u64 != expected.size
                || crate::sha256_bytes(&bytes) != expected.integrity
            {
                return Err(CoreError::Rejected(format!(
                    "upload member changed after planning: {}",
                    mapping.local_relative_path
                )));
            }
            Ok(PayloadMember {
                name: mapping.remote_member,
                bytes,
            })
        })
        .collect::<Result<_, CoreError>>()?;
    Ok(SavePayload {
        members,
        content: content.clone(),
    })
}

fn stage_payload(
    stage_root: &Path,
    request: &ExecutionRequest,
    payload: &SavePayload,
) -> Result<(), (RejectionCode, String)> {
    let members: BTreeMap<_, _> = payload
        .members
        .iter()
        .map(|member| (member.name.as_str(), member.bytes.as_slice()))
        .collect();
    if members.len() != payload.members.len() || members.len() != request.mappings.len() {
        return Err((
            RejectionCode::PayloadInvalid,
            "payload members do not match the adapter mapping".to_owned(),
        ));
    }
    for mapping in &request.mappings {
        let Some(bytes) = members.get(mapping.remote_member.as_str()) else {
            return Err((
                RejectionCode::PayloadInvalid,
                format!("payload member {:?} is missing", mapping.remote_member),
            ));
        };
        let destination = stage_root.join(&mapping.local_relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                (
                    RejectionCode::PayloadInvalid,
                    format!("cannot create staging directory: {error}"),
                )
            })?;
        }
        write_all_synced(&destination, bytes).map_err(|error| {
            (
                RejectionCode::PayloadInvalid,
                format!("cannot stage payload: {error}"),
            )
        })?;
    }
    Ok(())
}

fn commit_staged(
    stage_root: &Path,
    live_root: &Path,
    request: &ExecutionRequest,
    hook: &impl ExecutionHook,
) -> Result<(), CoreError> {
    let mut mappings = request.mappings.clone();
    mappings.sort_by(|left, right| left.local_relative_path.cmp(&right.local_relative_path));
    for (index, mapping) in mappings.iter().enumerate() {
        let source = stage_root.join(&mapping.local_relative_path);
        let destination = live_root.join(&mapping.local_relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("create save directory", parent, error))?;
        }
        hook.before_replace(index, &destination)?;
        replace_file(&source, &destination)?;
    }
    Ok(())
}

fn payload_size(payload: &SavePayload) -> Result<u64, CoreError> {
    payload.members.iter().try_fold(0_u64, |total, member| {
        let size = u64::try_from(member.bytes.len())
            .map_err(|_| CoreError::Rejected("payload member is too large".to_owned()))?;
        total
            .checked_add(size)
            .ok_or_else(|| CoreError::Rejected("payload size exceeds u64".to_owned()))
    })
}

fn write_transaction_manifest(stage_path: &Path, files: &[String]) -> Result<(), CoreError> {
    let bytes = serde_json::to_vec(&TransactionManifest {
        relative_files: files.to_vec(),
    })?;
    write_all_synced(&stage_path.join("transaction.json"), &bytes)
}

fn read_transaction_manifest(stage_path: &Path) -> Result<Vec<String>, CoreError> {
    let bytes = read_all(&stage_path.join("transaction.json"))?;
    let manifest: TransactionManifest = serde_json::from_slice(&bytes)?;
    Ok(manifest.relative_files)
}

fn remove_directory_if_exists(path: &Path) -> Result<(), CoreError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove directory", path, error)),
    }
}
