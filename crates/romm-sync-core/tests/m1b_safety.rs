use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    path::Path,
    sync::Mutex,
    time::Duration,
};

use fs2::FileExt;
use romm_sync_core::*;
use romm_sync_planner::*;
use rusqlite::Connection;
use tempfile::TempDir;

struct FakeRemote {
    observations: VecDeque<RemoteSaveState>,
    payload: Option<SavePayload>,
    upload_revision: String,
}

impl FakeRemote {
    fn new(state: RemoteSaveState, payload: Option<SavePayload>) -> Self {
        Self {
            observations: VecDeque::from([state]),
            payload,
            upload_revision: "uploaded-r2".to_owned(),
        }
    }

    fn with_observations(states: impl IntoIterator<Item = RemoteSaveState>) -> Self {
        Self {
            observations: states.into_iter().collect(),
            payload: None,
            upload_revision: "uploaded-r2".to_owned(),
        }
    }
}

impl RemoteTransport for FakeRemote {
    fn observe(&mut self, _key: &GameKey) -> Result<RemoteSaveState, CoreError> {
        if self.observations.len() > 1 {
            Ok(self.observations.pop_front().expect("queued observation"))
        } else {
            Ok(self
                .observations
                .front()
                .cloned()
                .unwrap_or(RemoteSaveState::Unavailable))
        }
    }

    fn download(
        &mut self,
        _key: &GameKey,
        _expected: &RemoteSaveSnapshot,
    ) -> Result<SavePayload, CoreError> {
        self.payload
            .clone()
            .ok_or_else(|| CoreError::Remote("no download payload".to_owned()))
    }

    fn upload(
        &mut self,
        key: &GameKey,
        payload: &SavePayload,
        _expected: &ExpectedRemoteState,
    ) -> Result<RemoteSaveSnapshot, CoreError> {
        let receipt = RemoteSaveSnapshot {
            content: payload.content.clone(),
            revision: self.upload_revision.clone(),
            slot: key.slot.clone(),
            extension: ".sav".to_owned(),
            size: payload
                .members
                .iter()
                .map(|member| member.bytes.len() as u64)
                .sum(),
        };
        self.observations = VecDeque::from([RemoteSaveState::Present(receipt.clone())]);
        Ok(receipt)
    }
}

struct SequenceActivity(Mutex<VecDeque<ActivityStatus>>);

impl SequenceActivity {
    fn new(states: impl IntoIterator<Item = ActivityStatus>) -> Self {
        Self(Mutex::new(states.into_iter().collect()))
    }
}

impl ActivityProbe for SequenceActivity {
    fn observe(&self, _key: &GameKey) -> Result<ActivityStatus, CoreError> {
        let mut states = self.0.lock().expect("activity lock");
        if states.len() > 1 {
            Ok(states.pop_front().expect("queued activity"))
        } else {
            Ok(*states.front().unwrap_or(&ActivityStatus::Unknown))
        }
    }
}

struct FixedSpace(u64);

impl SpaceProbe for FixedSpace {
    fn available_bytes(&self, _path: &Path) -> Result<u64, CoreError> {
        Ok(self.0)
    }
}

struct FailAt(usize);

impl ExecutionHook for FailAt {
    fn before_replace(&self, index: usize, _destination: &Path) -> Result<(), CoreError> {
        if index == self.0 {
            Err(CoreError::Injected(format!("replace {index}")))
        } else {
            Ok(())
        }
    }
}

fn adapter(platform: Platform, model: SaveModel) -> AdapterSnapshot {
    AdapterSnapshot {
        adapter_id: format!("{}-fixture", platform.slug()),
        adapter_revision: "m1b-v1".to_owned(),
        platform,
        status: AdapterStatus::Validated,
        save_model: model,
        compatibility: CompatibilityStatus::Compatible,
        can_upload: true,
        can_download: true,
        expected_slot: SlotId::from("autosave"),
    }
}

fn key(platform: Platform, rom_id: &str) -> GameKey {
    GameKey {
        platform,
        rom_id: rom_id.to_owned(),
        slot: SlotId::from("autosave"),
    }
}

fn local_group(root: &Path, files: &[&str]) -> SaveGroupSnapshot {
    let root_state = observe_root(root);
    let RootStatus::Available(root_snapshot) = root_state else {
        panic!("fixture root should be available");
    };
    let names: Vec<_> = files.iter().map(|value| (*value).to_owned()).collect();
    let LocalSaveState::Present(group) = observe_local_group(&root_snapshot, &names).unwrap()
    else {
        panic!("fixture group should be complete");
    };
    group
}

fn input(
    root: &Path,
    files: &[&str],
    remote: RemoteSaveState,
    baseline: BaselineState,
    model: SaveModel,
) -> ReconciliationInput {
    let local = if files.iter().all(|relative| root.join(relative).exists()) {
        LocalSaveState::Present(local_group(root, files))
    } else {
        LocalSaveState::Absent
    };
    ReconciliationInput {
        key: key(Platform::NintendoDs, "rom-1"),
        adapter: adapter(Platform::NintendoDs, model),
        identity: IdentityStatus::Verified,
        activity: ActivityStatus::Inactive,
        root: observe_root(root),
        local,
        remote,
        baseline,
    }
}

fn planned(input: ReconciliationInput) -> PlannedItem {
    plan(&PlannerInput {
        policy: SafetyPolicy::strict(),
        items: vec![input],
    })
    .items
    .into_iter()
    .next()
    .expect("one plan item")
}

fn remote_save(bytes: &[u8], revision: &str) -> RemoteSaveSnapshot {
    RemoteSaveSnapshot {
        content: sha256_bytes(bytes),
        revision: revision.to_owned(),
        slot: SlotId::from("autosave"),
        extension: ".sav".to_owned(),
        size: bytes.len() as u64,
    }
}

fn executor(temp: &TempDir) -> GuardedExecutor {
    GuardedExecutor::new(ExecutorConfig {
        lock_root: temp.path().join("locks"),
        backup_root: temp.path().join("backups"),
        backup_policy: BackupPolicy::default(),
        stability_delay: Duration::ZERO,
    })
}

fn mapping(local: &str, remote: &str) -> FileMapping {
    FileMapping {
        local_relative_path: local.to_owned(),
        remote_member: remote.to_owned(),
    }
}

#[test]
fn dry_run_discovers_hashes_without_writing() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"local-save").unwrap();
    let before = fs::read(root.join("game.srm")).unwrap();
    let specs = vec![DiscoverySpec {
        key: key(Platform::NintendoDs, "rom-1"),
        adapter: adapter(Platform::NintendoDs, SaveModel::SingleFile),
        identity: IdentityStatus::Verified,
        activity: ActivityStatus::Inactive,
        root: root.clone(),
        files: vec!["game.srm".to_owned()],
        remote: RemoteSaveState::Absent,
        baseline: BaselineState::None,
    }];

    let report = dry_run(&specs, Duration::ZERO)?;
    assert!(report.read_only);
    assert_eq!(report.items[0].action, "upload");
    assert_eq!(report.items[0].reason, "local_only_initial_upload");
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), before);
    assert_eq!(fs::read_dir(root).unwrap().count(), 1);
    Ok(())
}

#[test]
fn migration_is_transactional_and_preserves_a_backup() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("state.db");
    let connection = Connection::open(&database)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "wal_autocheckpoint", 0)?;
    connection.execute("CREATE TABLE legacy_marker(value TEXT NOT NULL)", [])?;
    connection.execute("INSERT INTO legacy_marker(value) VALUES ('preserved')", [])?;

    let store = StateStore::open(&database)?;
    drop(connection);
    assert_eq!(store.schema_version()?, STATE_SCHEMA_VERSION);
    let backup = migration_backup_path(&database, 0);
    assert!(backup.exists());
    let backup_connection =
        Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let marker: String =
        backup_connection.query_row("SELECT value FROM legacy_marker", [], |row| row.get(0))?;
    assert_eq!(marker, "preserved");
    Ok(())
}

#[test]
fn verified_download_backs_up_and_atomically_replaces() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"old").unwrap();
    let old_hash = sha256_bytes(b"old");
    let remote_snapshot = remote_save(b"new", "r2");
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Present(remote_snapshot.clone()),
        BaselineState::Known(BaselineSnapshot {
            local_content: old_hash.clone(),
            remote_content: old_hash,
            remote_revision: "r1".to_owned(),
        }),
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let payload = SavePayload {
        members: vec![PayloadMember {
            name: "game.sav".to_owned(),
            bytes: b"new".to_vec(),
        }],
        content: sha256_bytes(b"new"),
    };
    let mut remote = FakeRemote::new(RemoteSaveState::Present(remote_snapshot), Some(payload));
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut remote,
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;

    let ExecutionOutcome::Downloaded { backup_path, .. } = outcome else {
        panic!("download should succeed");
    };
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"new");
    let backup = backup_path.expect("existing save must be backed up");
    assert_eq!(fs::read(backup.join("game.srm")).unwrap(), b"old");
    assert_eq!(store.status()?.unfinished_executions, 0);
    Ok(())
}

#[test]
fn verified_upload_records_remote_revision_without_changing_local_save() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"local-save").unwrap();
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Absent,
        BaselineState::None,
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let mut remote = FakeRemote::new(RemoteSaveState::Absent, None);
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut remote,
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;

    assert!(matches!(
        outcome,
        ExecutionOutcome::Uploaded {
            remote_revision,
            ..
        } if remote_revision == "uploaded-r2"
    ));
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"local-save");
    assert_eq!(store.status()?.unfinished_executions, 0);
    Ok(())
}

#[test]
fn changed_local_hash_rejects_without_modification() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"old").unwrap();
    let old = sha256_bytes(b"old");
    let remote_snapshot = remote_save(b"remote", "r2");
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Present(remote_snapshot.clone()),
        BaselineState::Known(BaselineSnapshot {
            local_content: old.clone(),
            remote_content: old,
            remote_revision: "r1".to_owned(),
        }),
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    fs::write(root.join("game.srm"), b"changed-after-plan").unwrap();
    let mut remote = FakeRemote::new(RemoteSaveState::Present(remote_snapshot), None);
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut remote,
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::LocalChanged,
            ..
        }
    ));
    assert_eq!(
        fs::read(root.join("game.srm")).unwrap(),
        b"changed-after-plan"
    );
    Ok(())
}

#[test]
fn changed_remote_revision_and_mount_identity_reject() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"local").unwrap();
    let local = sha256_bytes(b"local");
    let planned_remote = remote_save(b"remote", "r2");
    let base = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Present(planned_remote.clone()),
        BaselineState::Known(BaselineSnapshot {
            local_content: local.clone(),
            remote_content: local,
            remote_revision: "r1".to_owned(),
        }),
        SaveModel::SingleFile,
    );
    let adapter_snapshot = base.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(base.clone()),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let mut changed_remote = planned_remote;
    changed_remote.revision = "r3".to_owned();
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut FakeRemote::new(RemoteSaveState::Present(changed_remote), None),
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::RemoteChanged,
            ..
        }
    ));

    let mut stale_mount = ExecutionRequest {
        planned: planned(base),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    stale_mount
        .planned
        .preconditions
        .as_mut()
        .expect("guards")
        .root
        .mount_identity = "replaced-mount".to_owned();
    let outcome = executor(&temp).execute(
        &store,
        &stale_mount,
        &adapter_snapshot,
        &mut FakeRemote::new(RemoteSaveState::Present(remote_save(b"remote", "r2")), None),
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::MountChanged,
            ..
        }
    ));
    Ok(())
}

#[test]
fn activity_race_after_staging_preserves_original() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"old").unwrap();
    let old = sha256_bytes(b"old");
    let remote_snapshot = remote_save(b"new", "r2");
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Present(remote_snapshot.clone()),
        BaselineState::Known(BaselineSnapshot {
            local_content: old.clone(),
            remote_content: old,
            remote_revision: "r1".to_owned(),
        }),
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let mut remote = FakeRemote::new(
        RemoteSaveState::Present(remote_snapshot),
        Some(SavePayload {
            members: vec![PayloadMember {
                name: "game.sav".to_owned(),
                bytes: b"new".to_vec(),
            }],
            content: sha256_bytes(b"new"),
        }),
    );
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut remote,
        &SequenceActivity::new([ActivityStatus::Inactive, ActivityStatus::Active]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::ActivityActive,
            ..
        }
    ));
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"old");
    Ok(())
}

#[test]
fn remote_revision_race_after_staging_preserves_original() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"old").unwrap();
    let old = sha256_bytes(b"old");
    let planned_remote = remote_save(b"new", "r2");
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Present(planned_remote.clone()),
        BaselineState::Known(BaselineSnapshot {
            local_content: old.clone(),
            remote_content: old,
            remote_revision: "r1".to_owned(),
        }),
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let mut changed_remote = planned_remote.clone();
    changed_remote.revision = "r3".to_owned();
    let mut remote = FakeRemote::with_observations([
        RemoteSaveState::Present(planned_remote),
        RemoteSaveState::Present(changed_remote),
    ]);
    remote.payload = Some(SavePayload {
        members: vec![PayloadMember {
            name: "game.sav".to_owned(),
            bytes: b"new".to_vec(),
        }],
        content: sha256_bytes(b"new"),
    });
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut remote,
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;

    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::RemoteChanged,
            ..
        }
    ));
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"old");
    Ok(())
}

#[test]
fn unknown_activity_and_insufficient_space_fail_closed() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"local").unwrap();
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Absent,
        BaselineState::None,
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let store = StateStore::open_in_memory()?;
    let guarded = executor(&temp);

    let activity_outcome = guarded.execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut FakeRemote::new(RemoteSaveState::Absent, None),
        &SequenceActivity::new([ActivityStatus::Unknown]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        activity_outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::ActivityUnknown,
            ..
        }
    ));

    let space_outcome = guarded.execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut FakeRemote::new(RemoteSaveState::Absent, None),
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(0),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        space_outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::InsufficientSpace,
            ..
        }
    ));
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"local");
    Ok(())
}

#[test]
fn truncated_payload_is_rejected_before_backup_or_replace() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"old").unwrap();
    let old = sha256_bytes(b"old");
    let remote_snapshot = remote_save(b"expected-complete", "r2");
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Present(remote_snapshot.clone()),
        BaselineState::Known(BaselineSnapshot {
            local_content: old.clone(),
            remote_content: old,
            remote_revision: "r1".to_owned(),
        }),
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let mut remote = FakeRemote::new(
        RemoteSaveState::Present(remote_snapshot.clone()),
        Some(SavePayload {
            members: vec![PayloadMember {
                name: "game.sav".to_owned(),
                bytes: b"truncated".to_vec(),
            }],
            content: remote_snapshot.content,
        }),
    );
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut remote,
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::PayloadInvalid,
            ..
        }
    ));
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"old");
    Ok(())
}

#[test]
fn failed_second_rename_rolls_back_complete_group() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    let remote_root = temp.path().join("remote-fixture");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&remote_root).unwrap();
    fs::write(root.join("a.dat"), b"old-a").unwrap();
    fs::write(root.join("b.dat"), b"old-b").unwrap();
    fs::write(remote_root.join("a.dat"), b"new-a").unwrap();
    fs::write(remote_root.join("b.dat"), b"new-b").unwrap();
    let old_group = local_group(&root, &["a.dat", "b.dat"]);
    let new_group = local_group(&remote_root, &["a.dat", "b.dat"]);
    let remote_snapshot = RemoteSaveSnapshot {
        content: new_group.content.clone(),
        revision: "r2".to_owned(),
        slot: SlotId::from("autosave"),
        extension: ".zip".to_owned(),
        size: new_group.total_size,
    };
    let observed = input(
        &root,
        &["a.dat", "b.dat"],
        RemoteSaveState::Present(remote_snapshot.clone()),
        BaselineState::Known(BaselineSnapshot {
            local_content: old_group.content.clone(),
            remote_content: old_group.content,
            remote_revision: "r1".to_owned(),
        }),
        SaveModel::MultiFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("a.dat", "a.dat"), mapping("b.dat", "b.dat")],
    };
    let mut remote = FakeRemote::new(
        RemoteSaveState::Present(remote_snapshot),
        Some(SavePayload {
            members: vec![
                PayloadMember {
                    name: "a.dat".to_owned(),
                    bytes: b"new-a".to_vec(),
                },
                PayloadMember {
                    name: "b.dat".to_owned(),
                    bytes: b"new-b".to_vec(),
                },
            ],
            content: new_group.content,
        }),
    );
    let store = StateStore::open_in_memory()?;
    let outcome = executor(&temp).execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut remote,
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &FailAt(1),
    )?;
    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::CommitFailed,
            ..
        }
    ));
    assert_eq!(fs::read(root.join("a.dat")).unwrap(), b"old-a");
    assert_eq!(fs::read(root.join("b.dat")).unwrap(), b"old-b");
    assert_eq!(store.status()?.unfinished_executions, 0);
    Ok(())
}

#[test]
fn interrupted_replacement_recovery_is_idempotent() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"old").unwrap();
    let group = local_group(&root, &["game.srm"]);
    let game_key = key(Platform::NintendoDs, "rom-1");
    let manager = BackupManager::new(temp.path().join("backups"), BackupPolicy::default());
    let backup = manager
        .create_routine_backup(1, &game_key, &root, &group.files)?
        .expect("backup");
    fs::write(root.join("game.srm"), b"partial-new").unwrap();
    let stage = root.join(".romm-autosync-staging").join("recovery");
    fs::create_dir_all(&stage).unwrap();
    fs::write(
        stage.join("transaction.json"),
        br#"{"relative_files":["game.srm"]}"#,
    )
    .unwrap();

    let store = StateStore::open_in_memory()?;
    let RootStatus::Available(root_snapshot) = observe_root(&root) else {
        panic!("root");
    };
    let journal = store.begin_execution(&game_key, PlanAction::Download, &root_snapshot)?;
    store.update_journal(
        journal,
        JournalState::Replacing,
        Some(&backup.directory),
        Some(&stage),
        None,
    )?;
    let guarded = executor(&temp);
    let activity = SequenceActivity::new([ActivityStatus::Inactive]);
    guarded.recover_interrupted(&store, &activity)?;
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"old");
    assert_eq!(store.status()?.unfinished_executions, 0);
    guarded.recover_interrupted(&store, &activity)?;
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"old");
    Ok(())
}

#[test]
fn interrupted_upload_recovery_requires_revalidation_without_local_rollback()
-> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"local").unwrap();
    let game_key = key(Platform::NintendoDs, "rom-1");
    let store = StateStore::open_in_memory()?;
    let RootStatus::Available(root_snapshot) = observe_root(&root) else {
        panic!("root");
    };
    let journal = store.begin_execution(&game_key, PlanAction::Upload, &root_snapshot)?;
    store.update_journal(journal, JournalState::Replacing, None, None, None)?;

    executor(&temp)
        .recover_interrupted(&store, &SequenceActivity::new([ActivityStatus::Inactive]))?;
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"local");
    assert_eq!(store.status()?.unfinished_executions, 0);
    Ok(())
}

#[test]
fn corrupted_backup_is_rejected_before_recovery_touches_live_save() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"old").unwrap();
    let group = local_group(&root, &["game.srm"]);
    let game_key = key(Platform::NintendoDs, "rom-1");
    let backup = BackupManager::new(temp.path().join("backups"), BackupPolicy::default())
        .create_routine_backup(1, &game_key, &root, &group.files)?
        .expect("backup");
    fs::write(backup.directory.join("game.srm"), b"corrupt").unwrap();
    fs::write(root.join("game.srm"), b"partial-new").unwrap();

    let store = StateStore::open_in_memory()?;
    let RootStatus::Available(root_snapshot) = observe_root(&root) else {
        panic!("root");
    };
    let journal = store.begin_execution(&game_key, PlanAction::Download, &root_snapshot)?;
    store.update_journal(
        journal,
        JournalState::Replacing,
        Some(&backup.directory),
        None,
        None,
    )?;
    let result = executor(&temp)
        .recover_interrupted(&store, &SequenceActivity::new([ActivityStatus::Inactive]));
    assert!(result.is_err());
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"partial-new");
    assert_eq!(store.status()?.unfinished_executions, 1);
    Ok(())
}

#[test]
fn per_game_lock_rejects_duplicate_work() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"local").unwrap();
    let observed = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Absent,
        BaselineState::None,
        SaveModel::SingleFile,
    );
    let adapter_snapshot = observed.adapter.clone();
    let request = ExecutionRequest {
        planned: planned(observed),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let guarded = executor(&temp);
    fs::create_dir_all(temp.path().join("locks")).unwrap();
    let lock_path = guarded.lock_path(&request.planned.key);
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    let store = StateStore::open_in_memory()?;
    let outcome = guarded.execute(
        &store,
        &request,
        &adapter_snapshot,
        &mut FakeRemote::new(RemoteSaveState::Absent, None),
        &SequenceActivity::new([ActivityStatus::Inactive]),
        &FixedSpace(u64::MAX),
        &NoopExecutionHook,
    )?;
    assert!(matches!(
        outcome,
        ExecutionOutcome::Rejected {
            code: RejectionCode::LockBusy,
            ..
        }
    ));
    Ok(())
}

#[test]
fn conflicts_and_ambiguous_identity_are_never_executable() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"local").unwrap();
    let mut conflict = input(
        &root,
        &["game.srm"],
        RemoteSaveState::Present(remote_save(b"remote", "r2")),
        BaselineState::None,
        SaveModel::SingleFile,
    );
    let adapter_snapshot = conflict.adapter.clone();
    let conflict_request = ExecutionRequest {
        planned: planned(conflict.clone()),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    assert_eq!(conflict_request.planned.action, PlanAction::Conflict);
    conflict.identity = IdentityStatus::Ambiguous;
    let ambiguous_request = ExecutionRequest {
        planned: planned(conflict),
        mappings: vec![mapping("game.srm", "game.sav")],
    };
    let store = StateStore::open_in_memory()?;
    for request in [conflict_request, ambiguous_request] {
        let outcome = executor(&temp).execute(
            &store,
            &request,
            &adapter_snapshot,
            &mut FakeRemote::new(RemoteSaveState::Unavailable, None),
            &SequenceActivity::new([ActivityStatus::Inactive]),
            &FixedSpace(u64::MAX),
            &NoopExecutionHook,
        )?;
        assert!(matches!(
            outcome,
            ExecutionOutcome::Rejected {
                code: RejectionCode::NonMutatingPlan,
                ..
            }
        ));
    }
    assert_eq!(fs::read(root.join("game.srm")).unwrap(), b"local");
    Ok(())
}

#[test]
fn routine_backups_rotate_but_conflicts_remain_protected() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"save").unwrap();
    let group = local_group(&root, &["game.srm"]);
    let game_key = key(Platform::NintendoDs, "rom-1");
    let manager = BackupManager::new(temp.path().join("backups"), BackupPolicy::default());
    let conflict = manager.create_conflict_archive("conflict-1", &game_key, &root, &group.files)?;
    let mut routine = Vec::new();
    for journal_id in 1..=12 {
        routine.push(
            manager
                .create_routine_backup(journal_id, &game_key, &root, &group.files)?
                .expect("routine backup"),
        );
    }
    assert!(!routine[0].directory.exists());
    assert!(!routine[1].directory.exists());
    assert!(routine[2..].iter().all(|record| record.directory.exists()));
    assert!(conflict.directory.exists());
    Ok(())
}

#[test]
fn backup_cap_never_prunes_the_in_flight_rollback_copy() -> Result<(), CoreError> {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("game.srm"), b"save").unwrap();
    let group = local_group(&root, &["game.srm"]);
    let manager = BackupManager::new(
        temp.path().join("backups"),
        BackupPolicy {
            routine_versions: 0,
            storage_cap_bytes: Some(0),
        },
    );
    let record = manager
        .create_routine_backup(1, &key(Platform::NintendoDs, "rom-1"), &root, &group.files)?
        .expect("transactional backup");
    assert!(record.directory.exists());
    assert_eq!(
        fs::read(record.directory.join("game.srm")).unwrap(),
        b"save"
    );
    Ok(())
}
