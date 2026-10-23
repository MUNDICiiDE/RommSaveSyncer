use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::{Duration, SystemTime},
};

use romm_sync_adapters::{
    AdapterResolution, BatteryProfile, DolphinCardConfig, GBA_BATTERY, NDS_BATTERY,
    ObservationContext, SnapshotMode, resolve_gci,
};
use romm_sync_core::{
    ActivityProbe, BackupPolicy, CoreError, ExecutionOutcome, ExecutionRequest, ExecutorConfig,
    GuardedExecutor, RemoteTransport, StateStore, SystemSpaceProbe, parse_platform, sha256_bytes,
};
use romm_sync_planner::{
    ActivityStatus, AdapterSnapshot, BaselineSnapshot, BaselineState, GameKey, IdentityStatus,
    LocalSaveState, PlanAction, PlannedItem, PlannerInput, Platform, ReasonCode, RemoteSaveState,
    SafetyPolicy, SlotId, plan,
};
use romm_sync_romm::{
    ClientConfig, CredentialStore, DeviceCredential, LocalSaveObservation, MemoryTrace,
    OperationAction, RemoteMapping, RomMClient, RomMTransport, SaveRecord, SyncOperation,
    UploadOutcome, md5_hex,
};
use serde::Serialize;
use time::{Duration as TimeDuration, OffsetDateTime, format_description::well_known::Rfc3339};

const CONFIRMATION: &str = "--confirm-disposable-romm-mutation";
const INSECURE_HTTP: &str = "--allow-insecure-http";
const INITIAL_UPLOAD: &str = "--confirm-initial-upload";
const SLOT: &str = "autosave";
const MAX_RETAINED_BEFORE: usize = 7;
const SYNTHETIC_BASELINE_REVISION: &str = "m3-qualification-synthetic-baseline";

type Failure = Box<dyn std::error::Error>;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("romm-m3-qualify: {error}");
            ExitCode::from(2)
        }
    }
}

struct Flags {
    allow_insecure_http: bool,
    initial_upload: bool,
}

fn run() -> Result<(), Failure> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if matches!(arguments.as_slice(), [command] if command == "--help" || command == "-h" || command == "help")
    {
        print_help();
        return Ok(());
    }
    let mut flags = Flags {
        allow_insecure_http: false,
        initial_upload: false,
    };
    let mut confirmed = false;
    let mut positional = Vec::new();
    for argument in &arguments {
        match argument.as_str() {
            CONFIRMATION => confirmed = true,
            INSECURE_HTTP => flags.allow_insecure_http = true,
            INITIAL_UPLOAD => flags.initial_upload = true,
            flag if flag.starts_with("--") => {
                print_help();
                return Err(format!("unknown flag {flag}").into());
            }
            value => positional.push(value),
        }
    }
    if !confirmed {
        print_help();
        return Err(format!("{CONFIRMATION} is required").into());
    }

    match positional.as_slice() {
        [
            command,
            platform,
            credential,
            rom_id,
            save_root,
            stem,
            workspace,
        ] if *command == "battery" => {
            let profile: &'static BatteryProfile = match parse_platform(platform)? {
                Platform::NintendoDs => &NDS_BATTERY,
                Platform::GameBoyAdvance => &GBA_BATTERY,
                _ => return Err("battery qualification supports only nds and gba".into()),
            };
            qualify(
                &Target::Battery { profile, stem },
                Path::new(credential),
                rom_id.parse()?,
                Path::new(save_root),
                Path::new(workspace),
                &flags,
            )
        }
        [command, credential, rom_id, gc_root, game_id, workspace] if *command == "gci" => qualify(
            &Target::Gci { game_id },
            Path::new(credential),
            rom_id.parse()?,
            Path::new(gc_root),
            Path::new(workspace),
            &flags,
        ),
        _ => {
            print_help();
            Err("invalid arguments".into())
        }
    }
}

enum Target<'a> {
    Battery {
        profile: &'static BatteryProfile,
        stem: &'a str,
    },
    Gci {
        game_id: &'a str,
    },
}

impl Target<'_> {
    fn resolve(
        &self,
        base: &Path,
        remote_name: Option<&str>,
    ) -> Result<AdapterResolution, Failure> {
        Ok(match self {
            Self::Battery { profile, stem } => profile.resolve(base, stem, remote_name)?,
            Self::Gci { game_id } => {
                resolve_gci(base, game_id, DolphinCardConfig::AUDITED, remote_name)?
            }
        })
    }

    fn default_emulator(&self) -> &'static str {
        match self {
            Self::Battery { profile, .. } => profile.romm_emulator,
            Self::Gci { .. } => "dolphin",
        }
    }
}

struct QualificationActivity;

impl ActivityProbe for QualificationActivity {
    /// Executions touch only copies inside the new workspace, which no
    /// emulator has open. Production activity probes arrive in M4.
    fn observe(&self, _key: &GameKey) -> Result<ActivityStatus, CoreError> {
        Ok(ActivityStatus::Inactive)
    }
}

struct Session<'a> {
    target: &'a Target<'a>,
    key: GameKey,
    rom_id: i64,
    workspace: PathBuf,
    credential: DeviceCredential,
    allow_insecure_http: bool,
    trace: Arc<MemoryTrace>,
    client: RomMClient,
    store: StateStore,
    executor: GuardedExecutor,
}

#[allow(clippy::too_many_lines)]
fn qualify(
    target: &Target<'_>,
    credential_path: &Path,
    rom_id: i64,
    live_base: &Path,
    workspace: &Path,
    flags: &Flags,
) -> Result<(), Failure> {
    let live = target.resolve(live_base, None)?;
    let platform = live.definition.platform;
    let key = GameKey {
        platform,
        rom_id: rom_id.to_string(),
        slot: SlotId::from(SLOT),
    };
    let members = capture_live(&live, &key)?;
    let [(_, local_bytes)] = members.as_slice() else {
        return Err("qualification requires exactly one adapter-owned save file".into());
    };
    let local_bytes = local_bytes.clone();
    let local_modified = fs::metadata(live.root.join(&live.files[0]))?
        .modified()
        .unwrap_or_else(|_| SystemTime::now());

    create_empty_workspace(workspace, live_base)?;
    let credential = CredentialStore::new(credential_path.to_path_buf()).load()?;
    let trace = Arc::new(MemoryTrace::default());
    let client = authenticated_client(&credential, flags.allow_insecure_http)?
        .with_trace_sink(trace.clone());
    let romm_version = client.ensure_compatible()?;
    let versions = slot_versions(&client, rom_id)?;
    let session = Session {
        target,
        key,
        rom_id,
        workspace: workspace.to_path_buf(),
        credential,
        allow_insecure_http: flags.allow_insecure_http,
        trace,
        client,
        store: StateStore::open(&workspace.join("state.db"))?,
        executor: GuardedExecutor::new(ExecutorConfig {
            lock_root: workspace.join("locks"),
            backup_root: workspace.join("backups"),
            backup_policy: BackupPolicy::default(),
            stability_delay: Duration::from_millis(250),
        }),
    };

    let (mode, remote_extension, passed_checks, expected_posts) = if flags.initial_upload {
        if !versions.is_empty() {
            return Err(
                "the autosave slot already has history; rerun without --confirm-initial-upload"
                    .into(),
            );
        }
        let extension =
            session.initial_upload_round_trip(&members, &local_bytes, local_modified)?;
        (
            "initial_upload",
            extension,
            vec![
                "live_source_validated_by_adapter",
                "initial_upload_planned_and_executed",
                "upload_verified",
                "repeated_upload_deduplicated",
                "initial_download_planned_and_executed",
                "download_validated_by_adapter",
                "download_confirmed_after_commit",
                "round_trip_bytes_identical",
            ],
            1,
        )
    } else {
        if versions.is_empty() {
            return Err(
                "the autosave slot is empty; use --confirm-initial-upload to qualify an initial upload"
                    .into(),
            );
        }
        if versions.len() > MAX_RETAINED_BEFORE {
            return Err(
                "the autosave slot has more than seven retained versions; qualification refuses to risk autocleanup"
                    .into(),
            );
        }
        let head = latest_version(&versions)?.clone();
        let extension =
            session.existing_head_round_trip(&head, &members, &local_bytes, local_modified)?;
        (
            "existing_head",
            extension,
            vec![
                "live_source_validated_by_adapter",
                "remote_head_validated_by_adapter",
                "first_sync_conflict_proven",
                "guarded_download_planned_and_executed",
                "download_backed_up_and_replaced",
                "download_confirmed_after_commit",
                "guarded_upload_planned_and_executed",
                "upload_verified",
                "repeated_upload_deduplicated",
                "remote_head_restored",
            ],
            2,
        )
    };

    let upload_post_count = session
        .trace
        .events()
        .iter()
        .filter(|event| event.method == "POST" && event.route == "/api/saves")
        .count();
    if upload_post_count != expected_posts {
        return Err(format!(
            "qualification issued {upload_post_count} upload POSTs; expected {expected_posts}"
        )
        .into());
    }
    for (name, bytes) in &members {
        if fs::read(live.root.join(name))? != *bytes {
            return Err("live source changed during qualification".into());
        }
    }
    let retained_versions_after = slot_versions(&session.client, rom_id)?.len();

    let mut passed_checks = passed_checks;
    passed_checks.push("live_source_unchanged");
    passed_checks.push("traces_sanitized");
    let report = QualificationReport {
        schema_version: 1,
        platform: platform.slug(),
        configuration_id: live.definition.configuration_id,
        adapter_revision: live.definition.revision,
        mode,
        romm_version,
        slot: SLOT,
        remote_extension,
        size: local_bytes.len() as u64,
        passed_checks,
        hashes_matched: true,
        retained_versions_before: versions.len(),
        retained_versions_after,
        upload_post_count,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

impl Session<'_> {
    #[allow(clippy::too_many_lines)]
    fn existing_head_round_trip(
        &self,
        head: &SaveRecord,
        members: &[(String, Vec<u8>)],
        local_bytes: &[u8],
        local_modified: SystemTime,
    ) -> Result<String, Failure> {
        let probe = self.target.resolve(
            &self.workspace.join("probe-does-not-exist"),
            Some(&head.file_name),
        )?;
        if !probe.accepts_remote_extension(&head.file_name) {
            return Err(
                "the RomM slot head uses a format this adapter does not accept; refusing to skip it"
                    .into(),
            );
        }
        let head_md5 = head
            .content_hash
            .clone()
            .ok_or("the selected remote head has no content hash")?;
        let local_md5 = md5_hex(local_bytes);
        if local_md5 == head_md5 {
            return Err(
                "local and remote bytes already match; choose two different valid disposable revisions"
                    .into(),
            );
        }
        let emulator = head
            .emulator
            .clone()
            .unwrap_or_else(|| self.target.default_emulator().to_owned());

        let negotiation = self.client.negotiate(
            vec![observation(
                self.rom_id,
                &head.file_name,
                local_bytes,
                &emulator,
                OffsetDateTime::from(local_modified),
            )?],
            Some(vec![self.rom_id]),
            Some(vec![emulator.clone()]),
        )?;
        let original = self
            .client
            .download_unconfirmed(head.clone(), negotiation.session_id)?;
        probe.validate_member(&original.bytes).map_err(|reason| {
            format!("remote head failed adapter validation: {}", reason.as_str())
        })?;
        write_new_synced(
            &self.workspace.join("original-remote-head.bin"),
            &original.bytes,
        )?;
        let original_sha = sha256_bytes(&original.bytes);

        // Baseline-free divergence must be a conflict regardless of timestamps.
        let conflict = self.stage_phase("conflict", members, Some(&head.file_name))?;
        let mut transport = self.transport(negotiation.session_id, &conflict, &emulator)?;
        let remote = transport.observe(&self.key)?;
        require_remote_content(&remote, &original_sha)?;
        let (planned, _) = self.plan_phase(&conflict, remote, BaselineState::None)?;
        require_plan(
            &planned,
            PlanAction::Conflict,
            ReasonCode::FirstSyncDiverged,
        )?;

        // Download: the workspace copy matches a synthetic baseline, so only
        // the remote side changed. The executor backs up, replaces, re-reads
        // and only then acknowledges the download to RomM.
        let download = self.stage_phase("download", members, Some(&head.file_name))?;
        let mut transport = self.transport(negotiation.session_id, &download, &emulator)?;
        let remote = transport.observe(&self.key)?;
        require_remote_content(&remote, &original_sha)?;
        let local_sha = sha256_bytes(local_bytes);
        let (planned, adapter) = self.plan_phase(
            &download,
            remote,
            BaselineState::Known(BaselineSnapshot {
                local_content: local_sha.clone(),
                remote_content: local_sha,
                remote_revision: SYNTHETIC_BASELINE_REVISION.to_owned(),
            }),
        )?;
        require_plan(&planned, PlanAction::Download, ReasonCode::RemoteChanged)?;
        let outcome = self.execute(&download, planned, &adapter, &mut transport)?;
        let ExecutionOutcome::Downloaded { backup_path, .. } = outcome else {
            return Err(format!(
                "guarded download did not commit: {}",
                outcome_name(&outcome)
            )
            .into());
        };
        let target = &download.mappings[0].local_relative_path;
        if fs::read(download.root.join(target))? != original.bytes {
            return Err("committed download differs from the verified remote head".into());
        }
        let backup = backup_path.ok_or("guarded download did not back up the replaced save")?;
        if fs::read(backup.join(target))? != local_bytes {
            return Err("download backup does not contain the replaced bytes".into());
        }
        self.client.complete_session(negotiation.session_id, 1, 0)?;

        // Upload: the workspace copy changed from the current head baseline.
        let upload_negotiation = self.client.negotiate(
            vec![observation(
                self.rom_id,
                &head.file_name,
                local_bytes,
                &emulator,
                after_server_update(head)?,
            )?],
            Some(vec![self.rom_id]),
            Some(vec![emulator.clone()]),
        )?;
        require_operation(
            &upload_negotiation.operations,
            self.rom_id,
            OperationAction::Upload,
        )?;
        let upload = self.stage_phase("upload", members, Some(&head.file_name))?;
        let mut transport = self.transport(upload_negotiation.session_id, &upload, &emulator)?;
        let remote = transport.observe(&self.key)?;
        let RemoteSaveState::Present(current) = &remote else {
            return Err("RomM head disappeared before the guarded upload".into());
        };
        let baseline = BaselineState::Known(BaselineSnapshot {
            local_content: current.content.clone(),
            remote_content: current.content.clone(),
            remote_revision: current.revision.clone(),
        });
        let (planned, adapter) = self.plan_phase(&upload, remote, baseline)?;
        require_plan(&planned, PlanAction::Upload, ReasonCode::LocalChanged)?;
        let outcome = self.execute(&upload, planned, &adapter, &mut transport)?;
        if !matches!(outcome, ExecutionOutcome::Uploaded { .. }) {
            return Err(
                format!("guarded upload did not commit: {}", outcome_name(&outcome)).into(),
            );
        }
        let uploaded_head = self
            .client
            .latest_save(self.rom_id, SLOT)?
            .ok_or("uploaded autosave head is missing")?;
        if uploaded_head.content_hash.as_deref() != Some(local_md5.as_str()) {
            return Err("uploaded autosave head failed MD5 verification".into());
        }
        let repeated = self.client.upload_autosave(
            self.rom_id,
            upload_negotiation.session_id,
            SLOT,
            &upload.upload_file_name,
            Some(&emulator),
            local_bytes.to_vec(),
        )?;
        if !matches!(repeated, UploadOutcome::AlreadyPresent(_)) {
            return Err("repeated qualification upload was not deduplicated".into());
        }
        self.client
            .complete_session(upload_negotiation.session_id, 1, 0)?;

        // Restore the retained original head.
        let restore_negotiation = self.client.negotiate(
            vec![observation(
                self.rom_id,
                &head.file_name,
                &original.bytes,
                &emulator,
                after_server_update(&uploaded_head)?,
            )?],
            Some(vec![self.rom_id]),
            None,
        )?;
        require_restore_operation(&restore_negotiation.operations, self.rom_id, &local_md5)?;
        self.client.upload_autosave(
            self.rom_id,
            restore_negotiation.session_id,
            SLOT,
            &head.file_name,
            Some(&emulator),
            original.bytes.clone(),
        )?;
        self.client
            .complete_session(restore_negotiation.session_id, 1, 0)?;
        let restored = self
            .client
            .latest_save(self.rom_id, SLOT)?
            .ok_or("restored autosave head is missing")?;
        if restored.content_hash.as_deref() != Some(head_md5.as_str()) {
            return Err("remote autosave head was not restored to its original bytes".into());
        }
        Ok(extension(&head.file_name))
    }

    fn initial_upload_round_trip(
        &self,
        members: &[(String, Vec<u8>)],
        local_bytes: &[u8],
        local_modified: SystemTime,
    ) -> Result<String, Failure> {
        let emulator = self.target.default_emulator().to_owned();
        let upload = self.stage_phase("upload", members, None)?;
        let negotiation = self.client.negotiate(
            vec![observation(
                self.rom_id,
                &upload.upload_file_name,
                local_bytes,
                &emulator,
                OffsetDateTime::from(local_modified),
            )?],
            Some(vec![self.rom_id]),
            Some(vec![emulator.clone()]),
        )?;
        let mut transport = self.transport(negotiation.session_id, &upload, &emulator)?;
        let remote = transport.observe(&self.key)?;
        if remote != RemoteSaveState::Absent {
            return Err("the autosave slot gained a head before the initial upload".into());
        }
        let (planned, adapter) = self.plan_phase(&upload, remote, BaselineState::None)?;
        require_plan(
            &planned,
            PlanAction::Upload,
            ReasonCode::LocalOnlyInitialUpload,
        )?;
        let outcome = self.execute(&upload, planned, &adapter, &mut transport)?;
        if !matches!(outcome, ExecutionOutcome::Uploaded { .. }) {
            return Err(
                format!("guarded upload did not commit: {}", outcome_name(&outcome)).into(),
            );
        }
        let local_md5 = md5_hex(local_bytes);
        let head = self
            .client
            .latest_save(self.rom_id, SLOT)?
            .ok_or("uploaded autosave head is missing")?;
        if head.content_hash.as_deref() != Some(local_md5.as_str()) {
            return Err("uploaded autosave head failed MD5 verification".into());
        }
        let repeated = self.client.upload_autosave(
            self.rom_id,
            negotiation.session_id,
            SLOT,
            &upload.upload_file_name,
            Some(&emulator),
            local_bytes.to_vec(),
        )?;
        if !matches!(repeated, UploadOutcome::AlreadyPresent(_)) {
            return Err("repeated qualification upload was not deduplicated".into());
        }
        self.client.complete_session(negotiation.session_id, 1, 0)?;

        let download_negotiation = self.client.negotiate(
            Vec::new(),
            Some(vec![self.rom_id]),
            Some(vec![emulator.clone()]),
        )?;
        let download = self.stage_phase("download", &[], Some(&head.file_name))?;
        let mut transport =
            self.transport(download_negotiation.session_id, &download, &emulator)?;
        let remote = transport.observe(&self.key)?;
        require_remote_content(&remote, &sha256_bytes(local_bytes))?;
        let (planned, adapter) = self.plan_phase(&download, remote, BaselineState::None)?;
        require_plan(
            &planned,
            PlanAction::Download,
            ReasonCode::RemoteOnlyInitialDownload,
        )?;
        let outcome = self.execute(&download, planned, &adapter, &mut transport)?;
        if !matches!(outcome, ExecutionOutcome::Downloaded { .. }) {
            return Err(format!(
                "guarded download did not commit: {}",
                outcome_name(&outcome)
            )
            .into());
        }
        let target = &download.mappings[0].local_relative_path;
        if fs::read(download.root.join(target))? != local_bytes {
            return Err("downloaded save differs from the uploaded bytes".into());
        }
        self.client
            .complete_session(download_negotiation.session_id, 1, 0)?;
        Ok(extension(&head.file_name))
    }

    /// Creates `<workspace>/<phase>` holding copies of the adapter-owned files.
    fn stage_phase(
        &self,
        phase: &str,
        members: &[(String, Vec<u8>)],
        remote_name: Option<&str>,
    ) -> Result<AdapterResolution, Failure> {
        let base = self.workspace.join(phase);
        fs::create_dir(&base)?;
        let empty = self.target.resolve(&base, remote_name)?;
        fs::create_dir_all(&empty.root)?;
        for (name, bytes) in members {
            write_new_synced(&empty.root.join(name), bytes)?;
        }
        self.target.resolve(&base, remote_name)
    }

    fn transport(
        &self,
        session_id: i64,
        resolution: &AdapterResolution,
        emulator: &str,
    ) -> Result<RomMTransport, Failure> {
        let client = authenticated_client(&self.credential, self.allow_insecure_http)?
            .with_trace_sink(self.trace.clone());
        let mapping = RemoteMapping {
            rom_id: self.rom_id,
            emulator: Some(emulator.to_owned()),
            remote_member: resolution.mappings[0].remote_member.clone(),
            upload_file_name: resolution.upload_file_name.clone(),
        };
        Ok(RomMTransport::new(
            client,
            session_id,
            BTreeMap::from([(self.key.clone(), mapping)]),
        ))
    }

    fn plan_phase(
        &self,
        resolution: &AdapterResolution,
        remote: RemoteSaveState,
        baseline: BaselineState,
    ) -> Result<(PlannedItem, AdapterSnapshot), Failure> {
        let observed = resolution.observe(&ObservationContext {
            key: self.key.clone(),
            identity: IdentityStatus::Verified,
            activity: ActivityStatus::Inactive,
            remote,
            baseline,
            stability_delay: Duration::from_millis(250),
            mode: SnapshotMode::Qualification,
        })?;
        if !observed.reasons.is_empty() {
            return Err(format!(
                "adapter blocked the phase: {}",
                reason_codes(&observed.reasons)
            )
            .into());
        }
        let adapter = observed.input.adapter.clone();
        let mut planned = plan(&PlannerInput {
            policy: SafetyPolicy::strict(),
            items: vec![observed.input],
        });
        Ok((planned.items.remove(0), adapter))
    }

    fn execute(
        &self,
        resolution: &AdapterResolution,
        planned: PlannedItem,
        adapter: &AdapterSnapshot,
        transport: &mut RomMTransport,
    ) -> Result<ExecutionOutcome, Failure> {
        Ok(self.executor.execute(
            &self.store,
            &ExecutionRequest {
                planned,
                mappings: resolution.mappings.clone(),
            },
            adapter,
            transport,
            &QualificationActivity,
            &SystemSpaceProbe,
            resolution,
        )?)
    }
}

/// Validates the live save through its adapter and copies its owned bytes.
fn capture_live(
    live: &AdapterResolution,
    key: &GameKey,
) -> Result<Vec<(String, Vec<u8>)>, Failure> {
    let observed = live.observe(&ObservationContext {
        key: key.clone(),
        identity: IdentityStatus::Verified,
        activity: ActivityStatus::Unknown,
        remote: RemoteSaveState::Unavailable,
        baseline: BaselineState::None,
        stability_delay: Duration::from_millis(250),
        mode: SnapshotMode::Production,
    })?;
    if !observed.reasons.is_empty() {
        return Err(format!(
            "the live save is not qualifiable: {}",
            reason_codes(&observed.reasons)
        )
        .into());
    }
    if !live.definition.qualifiable() {
        return Err("this adapter configuration has not passed fixture validation".into());
    }
    let LocalSaveState::Present(group) = &observed.input.local else {
        return Err("the live save is absent, incomplete, or malformed".into());
    };
    let mut members = Vec::new();
    for file in &group.files {
        let path = live.root.join(&file.relative_path);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err("the live save must be a regular non-symlink file".into());
        }
        let bytes = fs::read(&path)?;
        if sha256_bytes(&bytes) != file.integrity {
            return Err("the live save changed while it was being copied".into());
        }
        members.push((file.relative_path.clone(), bytes));
    }
    Ok(members)
}

fn require_plan(
    planned: &PlannedItem,
    action: PlanAction,
    reason: ReasonCode,
) -> Result<(), Failure> {
    if planned.action == action && planned.reason == reason {
        Ok(())
    } else {
        Err(format!(
            "pure planner returned {:?}/{} instead of {action:?}/{}",
            planned.action,
            planned.reason.as_str(),
            reason.as_str()
        )
        .into())
    }
}

fn require_remote_content(
    remote: &RemoteSaveState,
    expected: &romm_sync_planner::Fingerprint,
) -> Result<(), Failure> {
    match remote {
        RemoteSaveState::Present(snapshot) if snapshot.content == *expected => Ok(()),
        _ => Err("RomM head changed during qualification".into()),
    }
}

fn require_operation(
    operations: &[SyncOperation],
    rom_id: i64,
    action: OperationAction,
) -> Result<(), Failure> {
    if operations.iter().any(|operation| {
        operation.rom_id == rom_id
            && operation.slot.as_deref() == Some(SLOT)
            && operation.action == action
    }) {
        Ok(())
    } else {
        Err(format!("RomM did not negotiate the required {action:?} operation").into())
    }
}

fn require_restore_operation(
    operations: &[SyncOperation],
    rom_id: i64,
    expected_server_hash: &str,
) -> Result<(), Failure> {
    let operation = operations
        .iter()
        .find(|operation| operation.rom_id == rom_id && operation.slot.as_deref() == Some(SLOT))
        .ok_or("RomM did not return an autosave operation for guarded restore")?;
    if operation.server_content_hash.as_deref() != Some(expected_server_hash) {
        return Err("RomM restore negotiation did not revalidate the expected remote head".into());
    }
    match operation.action {
        OperationAction::Upload => Ok(()),
        OperationAction::NoOp if operation.reason == "No changes since last sync" => Ok(()),
        _ => Err(format!(
            "RomM did not permit guarded restore; observed={:?}",
            operation.action
        )
        .into()),
    }
}

fn slot_versions(client: &RomMClient, rom_id: i64) -> Result<Vec<SaveRecord>, Failure> {
    Ok(client
        .list_saves(rom_id)?
        .into_iter()
        .filter(|save| save.slot.as_deref() == Some(SLOT))
        .collect())
}

fn latest_version(versions: &[SaveRecord]) -> Result<&SaveRecord, Failure> {
    versions
        .iter()
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then(left.id.cmp(&right.id))
        })
        .ok_or_else(|| "no autosave versions".into())
}

fn authenticated_client(
    credential: &DeviceCredential,
    allow_insecure_http: bool,
) -> Result<RomMClient, Failure> {
    let config =
        ClientConfig::new(&credential.server_origin)?.with_insecure_http(allow_insecure_http);
    Ok(RomMClient::authenticated(config, credential.clone())?)
}

fn observation(
    rom_id: i64,
    file_name: &str,
    bytes: &[u8],
    emulator: &str,
    updated_at: OffsetDateTime,
) -> Result<LocalSaveObservation, Failure> {
    Ok(LocalSaveObservation {
        rom_id,
        file_name: file_name.to_owned(),
        slot: SLOT.to_owned(),
        emulator: Some(emulator.to_owned()),
        content_hash: md5_hex(bytes),
        updated_at: updated_at.format(&Rfc3339)?,
        file_size_bytes: bytes.len() as u64,
    })
}

fn after_server_update(save: &SaveRecord) -> Result<OffsetDateTime, Failure> {
    let updated_at = save
        .updated_at
        .as_deref()
        .ok_or("remote head has no updated_at timestamp")?;
    Ok(OffsetDateTime::parse(updated_at, &Rfc3339)? + TimeDuration::seconds(1))
}

fn create_empty_workspace(workspace: &Path, live_base: &Path) -> Result<(), Failure> {
    if workspace.exists() {
        return Err("qualification workspace must not already exist".into());
    }
    let live_base = live_base.canonicalize()?;
    let parent = workspace
        .parent()
        .ok_or("qualification workspace has no parent")?
        .canonicalize()?;
    let name = workspace
        .file_name()
        .ok_or("qualification workspace has no final path component")?;
    let workspace = parent.join(name);
    if workspace.starts_with(&live_base) || live_base.starts_with(&workspace) {
        return Err("qualification workspace must be outside the live save tree".into());
    }
    fs::create_dir(workspace)?;
    Ok(())
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), io::Error> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn extension(file_name: &str) -> String {
    Path::new(file_name)
        .extension()
        .map_or_else(String::new, |value| {
            value.to_string_lossy().to_ascii_lowercase()
        })
}

fn reason_codes(reasons: &[romm_sync_adapters::ReadinessReason]) -> String {
    reasons
        .iter()
        .map(|reason| reason.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn outcome_name(outcome: &ExecutionOutcome) -> String {
    match outcome {
        ExecutionOutcome::Uploaded { .. } => "uploaded".to_owned(),
        ExecutionOutcome::Downloaded { .. } => "downloaded".to_owned(),
        ExecutionOutcome::Rejected { code, .. } => format!("rejected:{}", code.as_str()),
    }
}

#[derive(Serialize)]
struct QualificationReport {
    schema_version: u32,
    platform: &'static str,
    configuration_id: &'static str,
    adapter_revision: &'static str,
    mode: &'static str,
    romm_version: String,
    slot: &'static str,
    remote_extension: String,
    size: u64,
    passed_checks: Vec<&'static str>,
    hashes_matched: bool,
    retained_versions_before: usize,
    retained_versions_after: usize,
    upload_post_count: usize,
}

fn print_help() {
    println!(
        "Disposable RomM M3 adapter qualification tool (Bazzite/Linux only)\n\n\
         Reuse the credential paired with romm-m2-qualify. Close the emulator first.\n\n\
         RetroArch battery save (melonDS DS or mGBA):\n\
           romm-m3-qualify battery <nds|gba> <credentials.json> <rom-id> <save-root> \\\n             <content-stem> <new-workspace> [--allow-insecure-http] \\\n             [--confirm-initial-upload] --confirm-disposable-romm-mutation\n\n\
         Dolphin GameCube GCI folder (Slot A GCI Folder, Slot B None):\n\
           romm-m3-qualify gci <credentials.json> <rom-id> <dolphin-gc-root> <game-id> \\\n             <new-workspace> [--allow-insecure-http] [--confirm-initial-upload] \\\n             --confirm-disposable-romm-mutation\n\n\
         The live save is validated by its adapter and copied; it is never written.\n\
         With an existing remote head, the tool proves the first-sync conflict, runs\n\
         a guarded download and a guarded upload through the M1B executor on\n\
         workspace copies, then restores the original head. With an empty slot,\n\
         --confirm-initial-upload uploads the copy (leaving one retained version)\n\
         and downloads it back into an empty workspace folder. The rom-id must be\n\
         the hash-verified RomM ROM for this save."
    );
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::create_empty_workspace;

    #[test]
    fn workspace_must_be_new_and_outside_the_live_tree() {
        let temp = TempDir::new().unwrap();
        let live = temp.path().join("saves");
        std::fs::create_dir(&live).unwrap();
        assert!(create_empty_workspace(&live.join("inside"), &live).is_err());
        assert!(create_empty_workspace(&live, &live).is_err());
        let sibling = temp.path().join("m3-workspace");
        create_empty_workspace(&sibling, &live).unwrap();
        assert!(sibling.is_dir());
        assert!(create_empty_workspace(&sibling, &live).is_err());
    }
}
