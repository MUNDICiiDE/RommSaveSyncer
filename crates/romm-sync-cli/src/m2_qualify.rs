use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    process::ExitCode,
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime},
};

use romm_sync_core::{observe_local_group, observe_root};
use romm_sync_planner::{
    ActivityStatus, AdapterSnapshot, AdapterStatus, BaselineState, CompatibilityStatus,
    Fingerprint, GameKey, HashAlgorithm, IdentityStatus, LocalSaveState, PlanAction, PlannerInput,
    Platform, ReconciliationInput, RemoteSaveSnapshot, RemoteSaveState, RootStatus, SafetyPolicy,
    SaveModel, SlotId, plan,
};
use romm_sync_romm::{
    ClientConfig, CredentialStore, DeviceAuthorizationRequest, DeviceCredential,
    LocalSaveObservation, MemoryTrace, OperationAction, RomMClient, SaveRecord, TokenPoll,
    UploadOutcome, md5_hex,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use time::{Duration as TimeDuration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const CONFIRMATION: &str = "--confirm-disposable-romm-mutation";
const INSECURE_HTTP: &str = "--allow-insecure-http";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("romm-m2-qualify: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    match arguments.as_slice() {
        [command, origin, credential] if command == "pair" => {
            pair(origin, Path::new(credential), false)
        }
        [command, origin, credential, flag] if command == "pair" && flag == INSECURE_HTTP => {
            pair(origin, Path::new(credential), true)
        }
        [command, credential, rom_id, source, workspace, confirmation]
            if command == "nds"
                && confirmation == CONFIRMATION
                && !arguments.iter().any(|argument| argument == INSECURE_HTTP) =>
        {
            qualify_nds(
                Path::new(credential),
                rom_id.parse()?,
                Path::new(source),
                Path::new(workspace),
                false,
            )
        }
        [
            command,
            credential,
            rom_id,
            source,
            workspace,
            insecure,
            confirmation,
        ] if command == "nds" && insecure == INSECURE_HTTP && confirmation == CONFIRMATION => {
            qualify_nds(
                Path::new(credential),
                rom_id.parse()?,
                Path::new(source),
                Path::new(workspace),
                true,
            )
        }
        [command, credential, rom_id, source, workspace, confirmation]
            if command == "recover-nds"
                && confirmation == CONFIRMATION
                && !arguments.iter().any(|argument| argument == INSECURE_HTTP) =>
        {
            recover_nds(
                Path::new(credential),
                rom_id.parse()?,
                Path::new(source),
                Path::new(workspace),
                false,
            )
        }
        [
            command,
            credential,
            rom_id,
            source,
            workspace,
            insecure,
            confirmation,
        ] if command == "recover-nds"
            && insecure == INSECURE_HTTP
            && confirmation == CONFIRMATION =>
        {
            recover_nds(
                Path::new(credential),
                rom_id.parse()?,
                Path::new(source),
                Path::new(workspace),
                true,
            )
        }
        [command] if matches!(command.as_str(), "help" | "-h" | "--help") => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err("invalid arguments".into())
        }
    }
}

fn pair(
    origin: &str,
    credential_path: &Path,
    allow_insecure_http: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = ClientConfig::new(origin)?.with_insecure_http(allow_insecure_http);
    let client = RomMClient::for_pairing(config)?;
    let authorization = client.begin_device_authorization(DeviceAuthorizationRequest {
        client_device_identifier: stable_device_id()?,
        name: "Beelink RomM Save Syncer".to_owned(),
        client: "romm-autosync".to_owned(),
        client_version: env!("CARGO_PKG_VERSION").to_owned(),
        platform: "bazzite".to_owned(),
        requested_scopes: Vec::new(),
    })?;
    println!("verification_path={}", authorization.verification_path);
    println!("user_code={}", authorization.user_code);
    println!("waiting_for_approval=true");
    io::stdout().flush()?;

    let deadline = Instant::now() + Duration::from_secs(authorization.expires_in);
    let interval = Duration::from_secs(authorization.interval.max(1));
    loop {
        if Instant::now() >= deadline {
            return Err("device authorization expired before approval".into());
        }
        thread::sleep(interval);
        match client.poll_device_token(&authorization.device_code)? {
            TokenPoll::Pending => {}
            TokenPoll::Authorized(credential) => {
                CredentialStore::new(credential_path.to_path_buf()).save(&credential)?;
                println!("credential_saved=true");
                println!("token_printed=false");
                return Ok(());
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn qualify_nds(
    credential_path: &Path,
    rom_id: i64,
    source: &Path,
    workspace: &Path,
    allow_insecure_http: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_source(source)?;
    create_empty_workspace(workspace, source)?;
    let source_copy = workspace.join("local-source.srm");
    copy_new_synced(source, &source_copy)?;
    let source_bytes = fs::read(&source_copy)?;

    let credential = CredentialStore::new(credential_path.to_path_buf()).load()?;
    let client = authenticated_client(&credential, allow_insecure_http)?;
    let trace = Arc::new(MemoryTrace::default());
    let client = client.with_trace_sink(trace.clone());
    let versions: Vec<_> = client
        .list_saves(rom_id)?
        .into_iter()
        .filter(|save| save.slot.as_deref() == Some("autosave"))
        .collect();
    if versions.is_empty() {
        return Err(
            "the selected ROM has no remote autosave to exercise download-first safety".into(),
        );
    }
    if versions.len() > 7 {
        return Err(
            "the autosave slot has more than seven retained versions; qualification refuses to risk autocleanup"
                .into(),
        );
    }
    let latest = latest_version(&versions)?.clone();
    if extension(&latest.file_name) != "sav" {
        return Err("the selected remote head is not a DS .sav".into());
    }
    let remote_md5 = latest
        .content_hash
        .clone()
        .ok_or("the selected remote head has no content hash")?;
    let source_md5 = md5_hex(&source_bytes);
    if source_md5 == remote_md5 {
        return Err(
            "local and remote bytes already match; choose two different valid disposable revisions"
                .into(),
        );
    }

    let emulator = latest
        .emulator
        .clone()
        .or_else(|| Some("melondsds".to_owned()));
    let initial = client.negotiate(
        vec![observation(
            rom_id,
            &latest.file_name,
            &source_copy,
            &source_bytes,
            emulator.clone(),
        )?],
        Some(vec![rom_id]),
        emulator.clone().map(|value| vec![value]),
    )?;
    let remote_download = client.download_unconfirmed(latest.clone(), initial.session_id)?;
    let downloaded_path = workspace.join("downloaded-remote.sav");
    write_new_synced(&downloaded_path, &remote_download.bytes)?;
    let downloaded_bytes = fs::read(&downloaded_path)?;
    if md5_hex(&downloaded_bytes) != remote_download.md5 {
        return Err("downloaded staging file failed read-back verification".into());
    }
    assert_first_sync_conflict(
        rom_id,
        workspace,
        &source_copy,
        &source_bytes,
        &remote_download.save,
        &downloaded_bytes,
    )?;
    client.confirm_download(remote_download.save.id, &remote_download.md5)?;
    client.complete_session(initial.session_id, 1, 0)?;

    let upload_negotiation = client.negotiate(
        vec![observation(
            rom_id,
            &latest.file_name,
            &source_copy,
            &source_bytes,
            emulator.clone(),
        )?],
        Some(vec![rom_id]),
        emulator.clone().map(|value| vec![value]),
    )?;
    require_operation(
        &upload_negotiation.operations,
        rom_id,
        OperationAction::Upload,
    )?;
    let first_upload = client.upload_autosave(
        rom_id,
        upload_negotiation.session_id,
        "autosave",
        &latest.file_name,
        source_bytes.clone(),
    )?;
    if !matches!(first_upload, UploadOutcome::Stored(_)) {
        return Err("first qualification upload unexpectedly already existed".into());
    }
    let repeated = client.upload_autosave(
        rom_id,
        upload_negotiation.session_id,
        "autosave",
        &latest.file_name,
        source_bytes,
    )?;
    if !matches!(repeated, UploadOutcome::AlreadyPresent(_)) {
        return Err("repeated qualification upload was not deduplicated".into());
    }
    client.complete_session(upload_negotiation.session_id, 1, 0)?;
    let uploaded_head = client
        .latest_save(rom_id, "autosave")?
        .ok_or("uploaded autosave head is missing")?;
    if uploaded_head.content_hash.as_deref() != Some(source_md5.as_str()) {
        return Err("uploaded autosave head failed MD5 verification".into());
    }

    let restore_negotiation = client.negotiate(
        vec![observation_at(
            rom_id,
            &latest.file_name,
            &downloaded_bytes,
            emulator.clone(),
            after_server_update(&uploaded_head)?,
        )?],
        Some(vec![rom_id]),
        emulator.map(|value| vec![value]),
    )?;
    require_operation(
        &restore_negotiation.operations,
        rom_id,
        OperationAction::Upload,
    )?;
    client.upload_autosave(
        rom_id,
        restore_negotiation.session_id,
        "autosave",
        &latest.file_name,
        downloaded_bytes,
    )?;
    client.complete_session(restore_negotiation.session_id, 1, 0)?;
    let restored = client
        .latest_save(rom_id, "autosave")?
        .ok_or("restored autosave head is missing")?;
    if restored.content_hash.as_deref() != Some(remote_md5.as_str()) {
        return Err("remote autosave head was not restored to its original bytes".into());
    }

    let events = trace.events();
    let upload_post_count = events
        .iter()
        .filter(|event| event.method == "POST" && event.route == "/api/saves")
        .count();
    if upload_post_count != 2 {
        return Err("qualification did not perform exactly one test upload and one restore".into());
    }
    let report = QualificationReport {
        schema_version: 1,
        platform: "nds",
        passed_checks: [
            "source_unchanged",
            "first_sync_conflict_proven",
            "download_staged_and_verified",
            "download_confirmed_after_commit",
            "upload_verified",
            "repeated_upload_deduplicated",
            "remote_head_restored",
            "traces_sanitized",
        ],
        retained_versions_before: versions.len(),
        upload_post_count,
    };
    if fs::read(source)? != fs::read(&source_copy)? {
        return Err("live source changed during qualification".into());
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn recover_nds(
    credential_path: &Path,
    rom_id: i64,
    source: &Path,
    workspace: &Path,
    allow_insecure_http: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_source(source)?;
    validate_recovery_workspace(workspace, source)?;
    let source_copy = workspace.join("local-source.srm");
    let downloaded_path = workspace.join("downloaded-remote.sav");
    let source_bytes = fs::read(&source_copy)?;
    if fs::read(source)? != source_bytes {
        return Err("live source changed since the interrupted qualification".into());
    }
    let downloaded_bytes = fs::read(&downloaded_path)?;
    let source_md5 = md5_hex(&source_bytes);
    let original_md5 = md5_hex(&downloaded_bytes);
    if source_md5 == original_md5 {
        return Err("recovery source and interrupted-test source are identical".into());
    }

    let credential = CredentialStore::new(credential_path.to_path_buf()).load()?;
    let client = authenticated_client(&credential, allow_insecure_http)?;
    let versions: Vec<_> = client
        .list_saves(rom_id)?
        .into_iter()
        .filter(|save| save.slot.as_deref() == Some("autosave"))
        .collect();
    if versions.len() > 8 {
        return Err(
            "the autosave slot has more than eight retained versions; recovery refuses unexpected remote state"
                .into(),
        );
    }
    let latest = latest_version(&versions)?.clone();
    if latest.content_hash.as_deref() != Some(source_md5.as_str()) {
        return Err(
            "remote head no longer matches the interrupted qualification upload; refusing recovery"
                .into(),
        );
    }
    if !versions
        .iter()
        .any(|save| save.content_hash.as_deref() == Some(original_md5.as_str()))
    {
        return Err("verified original remote version is no longer retained".into());
    }
    if extension(&latest.file_name) != "sav" {
        return Err("the selected remote head is not a DS .sav".into());
    }

    let emulator = latest
        .emulator
        .clone()
        .or_else(|| Some("melondsds".to_owned()));
    let negotiation = client.negotiate(
        vec![observation_at(
            rom_id,
            &latest.file_name,
            &downloaded_bytes,
            emulator.clone(),
            after_server_update(&latest)?,
        )?],
        Some(vec![rom_id]),
        emulator.map(|value| vec![value]),
    )?;
    require_operation(&negotiation.operations, rom_id, OperationAction::Upload)?;
    client.upload_autosave(
        rom_id,
        negotiation.session_id,
        "autosave",
        &latest.file_name,
        downloaded_bytes,
    )?;
    client.complete_session(negotiation.session_id, 1, 0)?;
    let restored = client
        .latest_save(rom_id, "autosave")?
        .ok_or("restored autosave head is missing")?;
    if restored.content_hash.as_deref() != Some(original_md5.as_str()) {
        return Err("remote autosave head was not restored to its original bytes".into());
    }
    if fs::read(source)? != source_bytes {
        return Err("live source changed during recovery".into());
    }

    println!("recovery_completed=true");
    println!("remote_head_restored=true");
    println!("live_source_unchanged=true");
    Ok(())
}

fn authenticated_client(
    credential: &DeviceCredential,
    allow_insecure_http: bool,
) -> Result<RomMClient, Box<dyn std::error::Error>> {
    let config =
        ClientConfig::new(&credential.server_origin)?.with_insecure_http(allow_insecure_http);
    Ok(RomMClient::authenticated(config, credential.clone())?)
}

fn observation(
    rom_id: i64,
    file_name: &str,
    path: &Path,
    bytes: &[u8],
    emulator: Option<String>,
) -> Result<LocalSaveObservation, Box<dyn std::error::Error>> {
    let modified = fs::metadata(path)?
        .modified()
        .unwrap_or_else(|_| SystemTime::now());
    observation_at(
        rom_id,
        file_name,
        bytes,
        emulator,
        OffsetDateTime::from(modified),
    )
}

fn observation_at(
    rom_id: i64,
    file_name: &str,
    bytes: &[u8],
    emulator: Option<String>,
    updated_at: OffsetDateTime,
) -> Result<LocalSaveObservation, Box<dyn std::error::Error>> {
    Ok(LocalSaveObservation {
        rom_id,
        file_name: file_name.to_owned(),
        slot: "autosave".to_owned(),
        emulator,
        content_hash: md5_hex(bytes),
        updated_at: updated_at.format(&Rfc3339)?,
        file_size_bytes: bytes.len() as u64,
    })
}

fn after_server_update(save: &SaveRecord) -> Result<OffsetDateTime, Box<dyn std::error::Error>> {
    let updated_at = save
        .updated_at
        .as_deref()
        .ok_or("remote head has no updated_at timestamp")?;
    Ok(OffsetDateTime::parse(updated_at, &Rfc3339)? + TimeDuration::seconds(1))
}

fn assert_first_sync_conflict(
    rom_id: i64,
    root: &Path,
    source: &Path,
    source_bytes: &[u8],
    remote: &SaveRecord,
    remote_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let root_state = observe_root(root);
    let RootStatus::Available(root_snapshot) = root_state.clone() else {
        return Err("qualification workspace is unavailable".into());
    };
    let relative = source.strip_prefix(root)?.to_string_lossy().into_owned();
    let local = observe_local_group(&root_snapshot, &[relative])?;
    if !matches!(local, LocalSaveState::Present(_)) {
        return Err("qualification source was not discoverable".into());
    }
    let key = GameKey {
        platform: Platform::NintendoDs,
        rom_id: rom_id.to_string(),
        slot: SlotId::from("autosave"),
    };
    let input = ReconciliationInput {
        key,
        adapter: AdapterSnapshot {
            adapter_id: "m2-nds-qualification".to_owned(),
            adapter_revision: "1".to_owned(),
            platform: Platform::NintendoDs,
            status: AdapterStatus::Validated,
            save_model: SaveModel::SingleFile,
            compatibility: CompatibilityStatus::Compatible,
            can_upload: true,
            can_download: true,
            expected_slot: SlotId::from("autosave"),
        },
        identity: IdentityStatus::Verified,
        activity: ActivityStatus::Inactive,
        root: root_state,
        local,
        remote: RemoteSaveState::Present(RemoteSaveSnapshot {
            content: sha256(remote_bytes),
            revision: remote.revision(),
            slot: SlotId::from("autosave"),
            extension: "sav".to_owned(),
            size: remote_bytes.len() as u64,
        }),
        baseline: BaselineState::None,
    };
    if sha256(source_bytes) == sha256(remote_bytes) {
        return Err("qualification revisions are not divergent".into());
    }
    let planned = plan(&PlannerInput {
        policy: SafetyPolicy::strict(),
        items: vec![input],
    });
    if planned.items.len() != 1 || planned.items[0].action != PlanAction::Conflict {
        return Err("pure planner did not preserve the first-sync divergence as a conflict".into());
    }
    Ok(())
}

fn require_operation(
    operations: &[romm_sync_romm::SyncOperation],
    rom_id: i64,
    action: OperationAction,
) -> Result<(), Box<dyn std::error::Error>> {
    if operations.iter().any(|operation| {
        operation.rom_id == rom_id
            && operation.slot.as_deref() == Some("autosave")
            && operation.action == action
    }) {
        Ok(())
    } else {
        let observed = operations
            .iter()
            .filter(|operation| {
                operation.rom_id == rom_id && operation.slot.as_deref() == Some("autosave")
            })
            .map(|operation| format!("{:?}", operation.action))
            .collect::<Vec<_>>()
            .join(",");
        Err(format!(
            "RomM did not negotiate the required {action:?} operation; observed={observed}"
        )
        .into())
    }
}

fn validate_source(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("source must be a regular non-symlink file".into());
    }
    if extension(path.to_string_lossy().as_ref()) != "srm" {
        return Err("Nintendo DS qualification source must use .srm".into());
    }
    if metadata.len() == 0 || metadata.len() > 128 * 1024 * 1024 {
        return Err("qualification source size is empty or unreasonable".into());
    }
    Ok(())
}

fn create_empty_workspace(
    workspace: &Path,
    source: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if workspace.exists() {
        return Err("qualification workspace must not already exist".into());
    }
    let source = source.canonicalize()?;
    let parent = workspace
        .parent()
        .ok_or("qualification workspace has no parent")?
        .canonicalize()?;
    let name = workspace
        .file_name()
        .ok_or("qualification workspace has no final path component")?;
    let workspace = parent.join(name);
    if workspace == source || source.starts_with(&workspace) {
        return Err("qualification workspace must not be or contain the live source".into());
    }
    fs::create_dir(workspace)?;
    Ok(())
}

fn validate_recovery_workspace(
    workspace: &Path,
    source: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(workspace)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("recovery workspace must be a regular directory".into());
    }
    let workspace = workspace.canonicalize()?;
    let source = source.canonicalize()?;
    if source.starts_with(&workspace) {
        return Err("recovery workspace must not contain the live source".into());
    }
    for name in ["local-source.srm", "downloaded-remote.sav"] {
        let path = workspace.join(name);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!("recovery file {name} must be a regular file").into());
        }
        if metadata.len() == 0 || metadata.len() > 128 * 1024 * 1024 {
            return Err(format!("recovery file {name} has an unreasonable size").into());
        }
    }
    Ok(())
}

fn copy_new_synced(source: &Path, destination: &Path) -> Result<(), io::Error> {
    write_new_synced(destination, &fs::read(source)?)
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), io::Error> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn stable_device_id() -> Result<String, Box<dyn std::error::Error>> {
    let machine_id = fs::read_to_string("/etc/machine-id")?;
    if machine_id.trim().is_empty() {
        return Err("/etc/machine-id is empty".into());
    }
    Ok(Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("romm-autosync:{}", machine_id.trim()).as_bytes(),
    )
    .to_string())
}

fn latest_version(versions: &[SaveRecord]) -> Result<&SaveRecord, Box<dyn std::error::Error>> {
    versions
        .iter()
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then(left.id.cmp(&right.id))
        })
        .ok_or_else(|| "no autosave versions".into())
}

fn sha256(bytes: &[u8]) -> Fingerprint {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(encoded, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Fingerprint::new(HashAlgorithm::Sha256, encoded)
}

fn extension(file_name: &str) -> String {
    Path::new(file_name)
        .extension()
        .map_or_else(String::new, |value| {
            value.to_string_lossy().to_ascii_lowercase()
        })
}

#[derive(Serialize)]
struct QualificationReport {
    schema_version: u32,
    platform: &'static str,
    passed_checks: [&'static str; 8],
    retained_versions_before: usize,
    upload_post_count: usize,
}

fn print_help() {
    println!(
        "Disposable RomM M2 qualification tool (Bazzite/Linux only)\n\n\
         Pair without exposing a token:\n\
           romm-m2-qualify pair <server-origin> <credentials.json> [--allow-insecure-http]\n\n\
         Qualify DS using a copied source and restore the original remote head:\n\
           romm-m2-qualify nds <credentials.json> <rom-id> <source.srm> <new-workspace> \\\n             [--allow-insecure-http] --confirm-disposable-romm-mutation\n\n\
         Recover the original remote head after an interrupted qualification:\n\
           romm-m2-qualify recover-nds <credentials.json> <rom-id> <source.srm> <existing-workspace> \\\n             [--allow-insecure-http] --confirm-disposable-romm-mutation\n\n\
         The NDS command never writes the source path. It performs one test upload,\n\
         restores the retained original head, and refuses slots with more than seven\n\
         existing versions."
    );
}

#[cfg(test)]
mod tests {
    use std::fs;

    use romm_sync_romm::SaveRecord;
    use tempfile::TempDir;
    use time::{Duration as TimeDuration, OffsetDateTime, format_description::well_known::Rfc3339};

    use super::{after_server_update, create_empty_workspace, validate_recovery_workspace};

    #[test]
    fn qualification_workspace_may_be_a_sibling_of_the_live_tree() {
        let temporary = TempDir::new().expect("temporary root");
        let live_directory = temporary.path().join("live");
        fs::create_dir(&live_directory).expect("live directory");
        let source = live_directory.join("game.srm");
        fs::write(&source, b"save").expect("source");
        let workspace = temporary.path().join("qualification");

        create_empty_workspace(&workspace, &source).expect("safe sibling workspace");

        assert!(workspace.is_dir());
        assert_eq!(fs::read(source).expect("source remains"), b"save");
    }

    #[test]
    fn qualification_workspace_must_be_new() {
        let temporary = TempDir::new().expect("temporary root");
        let source = temporary.path().join("game.srm");
        fs::write(&source, b"save").expect("source");
        let workspace = temporary.path().join("qualification");
        fs::create_dir(&workspace).expect("pre-existing workspace");

        assert!(create_empty_workspace(&workspace, &source).is_err());
    }

    #[test]
    fn recovery_requires_the_exact_retained_workspace_files() {
        let temporary = TempDir::new().expect("temporary root");
        let source = temporary.path().join("game.srm");
        fs::write(&source, b"live").expect("source");
        let workspace = temporary.path().join("qualification");
        fs::create_dir(&workspace).expect("workspace");
        fs::write(workspace.join("local-source.srm"), b"live").expect("source copy");
        assert!(validate_recovery_workspace(&workspace, &source).is_err());
        fs::write(workspace.join("downloaded-remote.sav"), b"remote").expect("downloaded remote");
        validate_recovery_workspace(&workspace, &source).expect("valid recovery workspace");
    }

    #[test]
    fn recovery_timestamp_is_later_than_the_authoritative_server_head() {
        let timestamp = "2026-10-14T22:51:35Z";
        let save = SaveRecord {
            id: 1,
            rom_id: 2,
            file_name: "game.sav".to_owned(),
            slot: Some("autosave".to_owned()),
            emulator: Some("melondsds".to_owned()),
            content_hash: Some("hash".to_owned()),
            file_size: Some(8),
            updated_at: Some(timestamp.to_owned()),
            device_id: None,
            origin_device: None,
        };
        let original = OffsetDateTime::parse(timestamp, &Rfc3339).expect("timestamp");
        let later = after_server_update(&save).expect("later timestamp");
        assert_eq!(later - original, TimeDuration::seconds(1));
    }
}
