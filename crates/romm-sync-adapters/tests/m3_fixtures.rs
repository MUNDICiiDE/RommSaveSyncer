//! Synthetic M3 adapter fixtures: normal, missing, malformed, conflicting,
//! multi-file, region-mismatched and multi-disc cases for each adapter class.
//! Every byte is generated in a temporary directory; no real save is opened.

use std::{fs, path::Path, time::Duration};

use romm_sync_adapters::*;
use romm_sync_core::*;
use romm_sync_planner::*;
use tempfile::TempDir;

fn key(platform: Platform) -> GameKey {
    GameKey {
        platform,
        rom_id: "42".to_owned(),
        slot: SlotId::from("autosave"),
    }
}

fn context(
    platform: Platform,
    remote: RemoteSaveState,
    baseline: BaselineState,
    mode: SnapshotMode,
) -> ObservationContext {
    ObservationContext {
        key: key(platform),
        identity: IdentityStatus::Verified,
        activity: ActivityStatus::Inactive,
        remote,
        baseline,
        stability_delay: Duration::ZERO,
        mode,
    }
}

fn remote(bytes: &[u8], extension: &str, revision: &str) -> RemoteSaveState {
    RemoteSaveState::Present(RemoteSaveSnapshot {
        content: sha256_bytes(bytes),
        revision: revision.to_owned(),
        slot: SlotId::from("autosave"),
        extension: extension.to_owned(),
        size: bytes.len() as u64,
    })
}

fn plan_one(input: ReconciliationInput) -> PlannedItem {
    plan(&PlannerInput {
        policy: SafetyPolicy::strict(),
        items: vec![input],
    })
    .items
    .remove(0)
}

fn observe(resolution: &AdapterResolution, context: &ObservationContext) -> ObservedSave {
    resolution.observe(context).expect("fixture observation")
}

fn gci(game_id: &str, blocks: u16, fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; 0x40 + usize::from(blocks) * 0x2000];
    bytes[..6].copy_from_slice(game_id.as_bytes());
    bytes[6] = 0xFF;
    bytes[7] = 0;
    bytes[0x08..0x28].fill(0);
    bytes[0x08..0x08 + 9].copy_from_slice(b"fixture01");
    bytes[0x38..0x3A].copy_from_slice(&blocks.to_be_bytes());
    bytes[0x3A..0x3C].copy_from_slice(&[0xFF, 0xFF]);
    bytes
}

fn card_a(gc_root: &Path, region: &str) -> std::path::PathBuf {
    let folder = gc_root.join(region).join("Card A");
    fs::create_dir_all(&folder).unwrap();
    folder
}

// ---------------------------------------------------------------- battery

#[test]
fn battery_normal_save_is_single_file_and_production_stays_disabled() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Game (USA).srm"), vec![1; 8192]).unwrap();
    let resolution = NDS_BATTERY
        .resolve(temp.path(), "Game (USA)", Some("Game (USA).sav"))
        .unwrap();
    assert_eq!(resolution.save_model, SaveModel::SingleFile);
    assert_eq!(resolution.files, ["Game (USA).srm"]);
    assert_eq!(resolution.mappings[0].remote_member, "Game (USA).sav");
    assert_eq!(resolution.romm_emulator, "melondsds");

    let production = observe(
        &resolution,
        &context(
            Platform::NintendoDs,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Production,
        ),
    );
    assert!(matches!(production.input.local, LocalSaveState::Present(_)));
    let planned = plan_one(production.input);
    assert_eq!(planned.action, PlanAction::Unsupported);
    assert_eq!(planned.reason, ReasonCode::AdapterPendingValidation);

    let qualification = observe(
        &resolution,
        &context(
            Platform::NintendoDs,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert_eq!(
        qualification.input.adapter.adapter_id,
        "retroarch-melondsds-nds+qualification"
    );
    let planned = plan_one(qualification.input);
    assert_eq!(planned.action, PlanAction::Upload);
    assert!(planned.preconditions.is_some());
}

#[test]
fn battery_missing_save_downloads_into_the_srm_name() {
    let temp = TempDir::new().unwrap();
    let resolution = GBA_BATTERY.resolve(temp.path(), "Homebrew", None).unwrap();
    assert_eq!(resolution.mappings[0].local_relative_path, "Homebrew.srm");
    assert_eq!(resolution.mappings[0].remote_member, "Homebrew.sav");
    let observed = observe(
        &resolution,
        &context(
            Platform::GameBoyAdvance,
            remote(&vec![7; 32768], "sav", "r1"),
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert_eq!(observed.input.local, LocalSaveState::Absent);
    let planned = plan_one(observed.input);
    assert_eq!(planned.action, PlanAction::Download);
    assert_eq!(planned.reason, ReasonCode::RemoteOnlyInitialDownload);
}

#[test]
fn battery_malformed_size_is_never_executable() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Homebrew.srm"), vec![1; 1000]).unwrap();
    let resolution = GBA_BATTERY.resolve(temp.path(), "Homebrew", None).unwrap();
    let observed = observe(
        &resolution,
        &context(
            Platform::GameBoyAdvance,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert_eq!(observed.input.local, LocalSaveState::Malformed);
    assert!(
        observed
            .reasons
            .contains(&ReadinessReason::BatterySizeUnrecognized)
    );
    let planned = plan_one(observed.input);
    assert_eq!(planned.reason, ReasonCode::LocalGroupMalformed);
    assert!(!planned.action.is_mutation());
}

#[test]
fn battery_conflict_preserves_both_sides() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Game.srm"), vec![2; 8192]).unwrap();
    let resolution = NDS_BATTERY.resolve(temp.path(), "Game", None).unwrap();
    let base = sha256_bytes(&[1; 8192]);
    for baseline in [
        BaselineState::None,
        BaselineState::Known(BaselineSnapshot {
            local_content: base.clone(),
            remote_content: base,
            remote_revision: "r1".to_owned(),
        }),
    ] {
        let observed = observe(
            &resolution,
            &context(
                Platform::NintendoDs,
                remote(&[3; 8192], "sav", "r2"),
                baseline,
                SnapshotMode::Qualification,
            ),
        );
        let planned = plan_one(observed.input);
        assert_eq!(planned.action, PlanAction::Conflict);
        assert!(planned.preconditions.is_none());
    }
}

#[test]
fn battery_companions_block_and_states_and_artifacts_are_excluded() {
    let temp = TempDir::new().unwrap();
    for name in [
        "Game.srm",
        "Game.sav",
        "Game.state",
        "Game.state1.png",
        "Game.state.auto",
        "Game Two.srm",
        "Game.v2.srm",
    ] {
        fs::write(temp.path().join(name), vec![1; 8192]).unwrap();
    }
    let ds = NDS_BATTERY.resolve(temp.path(), "Game", None).unwrap();
    assert_eq!(ds.files, ["Game.srm"]);
    assert_eq!(ds.blocking, Vec::new());
    for diagnostic in [
        "excluded_test_artifact",
        "other_content_ignored",
        "save_state_excluded",
    ] {
        assert!(ds.diagnostics.iter().any(|value| value == diagnostic));
    }

    // GBA has no M0 `.sav` artifact exclusion, so the same file is unknown.
    let gba = GBA_BATTERY.resolve(temp.path(), "Game", None).unwrap();
    assert!(
        gba.blocking
            .contains(&ReadinessReason::UnrecognizedCompanion)
    );
    assert_eq!(gba.compatibility, CompatibilityStatus::Unknown);
    let planned = plan_one(
        observe(
            &gba,
            &context(
                Platform::GameBoyAdvance,
                RemoteSaveState::Absent,
                BaselineState::None,
                SnapshotMode::Qualification,
            ),
        )
        .input,
    );
    assert_eq!(planned.reason, ReasonCode::UnknownCompatibility);

    fs::write(temp.path().join("Game.rtc"), [0; 16]).unwrap();
    let multi = NDS_BATTERY.resolve(temp.path(), "Game", None).unwrap();
    assert_eq!(multi.save_model, SaveModel::MultiFile);
    assert_eq!(multi.files, ["Game.srm", "Game.rtc"]);
    assert!(
        multi
            .blocking
            .contains(&ReadinessReason::CompanionFileUnsupported)
    );
    let observed = observe(
        &multi,
        &context(
            Platform::NintendoDs,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    let planned = plan_one(observed.input);
    assert_eq!(planned.reason, ReasonCode::IncompatibleFormat);
}

#[test]
fn battery_incompatible_remote_head_blocks_instead_of_being_skipped() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Game.srm"), vec![1; 8192]).unwrap();
    let resolution = NDS_BATTERY.resolve(temp.path(), "Game", None).unwrap();
    for (head, reason) in [
        (
            remote(&[1; 100], "zip", "r1"),
            ReadinessReason::RemoteFormatUnsupported,
        ),
        (
            remote(&[1; 100], ".sav", "r1"),
            ReadinessReason::RemoteSizeInvalid,
        ),
    ] {
        let observed = observe(
            &resolution,
            &context(
                Platform::NintendoDs,
                head,
                BaselineState::None,
                SnapshotMode::Qualification,
            ),
        );
        assert!(observed.reasons.contains(&reason));
        let planned = plan_one(observed.input);
        assert_eq!(planned.reason, ReasonCode::IncompatibleFormat);
    }
}

#[test]
fn battery_missing_root_defers_and_unsafe_stems_are_rejected() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("not-mounted");
    let resolution = NDS_BATTERY.resolve(&missing, "Game", None).unwrap();
    let planned = plan_one(
        observe(
            &resolution,
            &context(
                Platform::NintendoDs,
                remote(&[1; 8192], "sav", "r1"),
                BaselineState::None,
                SnapshotMode::Qualification,
            ),
        )
        .input,
    );
    assert_eq!(planned.reason, ReasonCode::RootMissing);
    for stem in ["", "../Game", "dir/Game", ".hidden"] {
        assert!(NDS_BATTERY.resolve(temp.path(), stem, None).is_err());
    }
}

// ---------------------------------------------------------------- GCI

#[test]
fn gci_header_structure_is_validated() {
    let valid = gci("GKYE01", 2, 0xAA);
    assert_eq!(valid.len(), 16_448);
    let header = parse_gci(&valid).unwrap();
    assert_eq!(header.block_count, 2);
    assert_eq!(header.internal_name, "fixture01");

    let mut truncated = valid.clone();
    truncated.pop();
    assert_eq!(parse_gci(&truncated), Err(ReadinessReason::GciMalformed));
    assert_eq!(
        parse_gci(&valid[..0x20]),
        Err(ReadinessReason::GciMalformed)
    );
    let mut zero_blocks = gci("GKYE01", 1, 0);
    zero_blocks[0x38..0x3A].copy_from_slice(&0_u16.to_be_bytes());
    assert_eq!(parse_gci(&zero_blocks), Err(ReadinessReason::GciMalformed));
    let mut lowercase = valid.clone();
    lowercase[0] = b'g';
    assert_eq!(parse_gci(&lowercase), Err(ReadinessReason::GciMalformed));

    let game = GameId::parse("GKYE01").unwrap();
    assert_eq!(validate_gci_owner(&valid, &game), Ok(()));
    assert_eq!(
        validate_gci_owner(&gci("GALE01", 2, 0), &game),
        Err(ReadinessReason::GciOwnerMismatch)
    );
    assert!(GameId::parse("gkye01").is_err());
    assert!(GameId::parse("GKYE0").is_err());
}

#[test]
fn gci_normal_save_is_owned_by_header_not_filename() {
    let temp = TempDir::new().unwrap();
    let folder = card_a(temp.path(), "USA");
    fs::write(folder.join("misleading-name.gci"), gci("GKYE01", 2, 1)).unwrap();
    fs::write(folder.join("01-GKYE-looks-owned.gci"), gci("GALE01", 1, 2)).unwrap();
    fs::write(folder.join("unrelated.raw"), [0; 68]).unwrap();

    let resolution = resolve_gci(temp.path(), "GKYE01", DolphinCardConfig::AUDITED, None).unwrap();
    assert_eq!(resolution.root, folder);
    assert_eq!(resolution.files, ["misleading-name.gci"]);
    assert_eq!(resolution.save_model, SaveModel::SingleFile);
    assert_eq!(resolution.upload_file_name, "misleading-name.gci");
    assert!(
        resolution
            .diagnostics
            .iter()
            .any(|value| value == "non_gci_entry_ignored")
    );
    let planned = plan_one(
        observe(
            &resolution,
            &context(
                Platform::GameCube,
                RemoteSaveState::Absent,
                BaselineState::None,
                SnapshotMode::Qualification,
            ),
        )
        .input,
    );
    assert_eq!(planned.action, PlanAction::Upload);
}

#[test]
fn gci_missing_save_uses_safe_remote_name_or_placeholder() {
    let temp = TempDir::new().unwrap();
    card_a(temp.path(), "USA");
    let named = resolve_gci(
        temp.path(),
        "GKYE01",
        DolphinCardConfig::AUDITED,
        Some("01-GKYE-KirbyAirRide.gci"),
    )
    .unwrap();
    assert_eq!(named.files, ["01-GKYE-KirbyAirRide.gci"]);
    for unsafe_name in ["../escape.gci", "nested/name.gci", "save.zip"] {
        let fallback = resolve_gci(
            temp.path(),
            "GKYE01",
            DolphinCardConfig::AUDITED,
            Some(unsafe_name),
        )
        .unwrap();
        assert_eq!(fallback.files, ["01-GKYE-romm-autosync.gci"]);
    }
    let observed = observe(
        &named,
        &context(
            Platform::GameCube,
            remote(&gci("GKYE01", 2, 9), "gci", "r1"),
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert_eq!(observed.input.local, LocalSaveState::Absent);
    assert_eq!(plan_one(observed.input).action, PlanAction::Download);
}

#[test]
fn gci_malformed_entry_makes_folder_ownership_unprovable() {
    let temp = TempDir::new().unwrap();
    let folder = card_a(temp.path(), "USA");
    fs::write(folder.join("owned.gci"), gci("GKYE01", 2, 1)).unwrap();
    let mut broken = gci("GKYE01", 1, 1);
    broken.truncate(5000);
    fs::write(folder.join("broken.gci"), broken).unwrap();
    let resolution = resolve_gci(temp.path(), "GKYE01", DolphinCardConfig::AUDITED, None).unwrap();
    assert!(resolution.blocking.contains(&ReadinessReason::GciMalformed));
    let observed = observe(
        &resolution,
        &context(
            Platform::GameCube,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert_eq!(observed.input.local, LocalSaveState::Malformed);
    assert_eq!(
        plan_one(observed.input).reason,
        ReasonCode::LocalGroupMalformed
    );
}

#[test]
fn gci_multi_file_group_is_detected_and_blocked() {
    let temp = TempDir::new().unwrap();
    let folder = card_a(temp.path(), "USA");
    fs::write(folder.join("01-GKYE-a.gci"), gci("GKYE01", 2, 1)).unwrap();
    fs::write(folder.join("01-GKYE-b.gci"), gci("GKYE01", 1, 2)).unwrap();
    let resolution = resolve_gci(temp.path(), "GKYE01", DolphinCardConfig::AUDITED, None).unwrap();
    assert_eq!(resolution.save_model, SaveModel::MultiFile);
    assert_eq!(resolution.files, ["01-GKYE-a.gci", "01-GKYE-b.gci"]);
    assert!(
        resolution
            .blocking
            .contains(&ReadinessReason::MultiGciGroup)
    );
    let observed = observe(
        &resolution,
        &context(
            Platform::GameCube,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    let LocalSaveState::Present(group) = &observed.input.local else {
        panic!("complete multi-GCI group should still be observable");
    };
    assert_eq!(group.files.len(), 2);
    assert_eq!(
        plan_one(observed.input).reason,
        ReasonCode::IncompatibleFormat
    );
}

#[test]
fn gci_region_mismatch_and_unknown_region_are_not_attributed() {
    let temp = TempDir::new().unwrap();
    let usa = card_a(temp.path(), "USA");
    // PAL save of the same title placed in the USA folder: not this game.
    fs::write(usa.join("01-GKYP-pal.gci"), gci("GKYP01", 2, 1)).unwrap();
    let eur = card_a(temp.path(), "EUR");
    fs::write(eur.join("01-GKYP-pal.gci"), gci("GKYP01", 2, 1)).unwrap();

    let ntsc = resolve_gci(temp.path(), "GKYE01", DolphinCardConfig::AUDITED, None).unwrap();
    assert!(
        ntsc.diagnostics
            .iter()
            .any(|value| value == "same_title_other_region_gci_ignored")
    );
    let observed = observe(
        &ntsc,
        &context(
            Platform::GameCube,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert_eq!(observed.input.local, LocalSaveState::Absent);

    let pal = resolve_gci(temp.path(), "GKYP01", DolphinCardConfig::AUDITED, None).unwrap();
    assert_eq!(pal.root, eur);
    assert_eq!(pal.files, ["01-GKYP-pal.gci"]);

    let korean = resolve_gci(temp.path(), "GKYK01", DolphinCardConfig::AUDITED, None).unwrap();
    assert!(
        korean
            .blocking
            .contains(&ReadinessReason::GciRegionUnrecognized)
    );
    assert_eq!(korean.compatibility, CompatibilityStatus::Incompatible);

    // A downloaded PAL payload must never be accepted for the NTSC game.
    let wrong_region = remote(&gci("GKYP01", 2, 1), "gci", "r1");
    let observed = observe(
        &ntsc,
        &context(
            Platform::GameCube,
            wrong_region,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert_eq!(plan_one(observed.input).action, PlanAction::Download);
    assert_eq!(
        ntsc.validate_member(&gci("GKYP01", 2, 1)),
        Err(ReadinessReason::GciOwnerMismatch)
    );
}

#[test]
fn gci_unaudited_slot_configuration_and_zip_heads_block() {
    let temp = TempDir::new().unwrap();
    let folder = card_a(temp.path(), "USA");
    fs::write(folder.join("owned.gci"), gci("GKYE01", 2, 1)).unwrap();
    let slot_b = resolve_gci(
        temp.path(),
        "GKYE01",
        DolphinCardConfig {
            slot_a: CardDevice::GciFolder,
            slot_b: CardDevice::RawCard,
        },
        None,
    )
    .unwrap();
    assert!(
        slot_b
            .blocking
            .contains(&ReadinessReason::DolphinSlotConfigurationUnsupported)
    );

    let resolution = resolve_gci(temp.path(), "GKYE01", DolphinCardConfig::AUDITED, None).unwrap();
    let observed = observe(
        &resolution,
        &context(
            Platform::GameCube,
            remote(&[0; 2048], "zip", "foreign-zip-head"),
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    assert!(
        observed
            .reasons
            .contains(&ReadinessReason::RemoteFormatUnsupported)
    );
    assert_eq!(
        plan_one(observed.input).reason,
        ReasonCode::IncompatibleFormat
    );
}

#[test]
fn gci_conflict_never_produces_a_write() {
    let temp = TempDir::new().unwrap();
    let folder = card_a(temp.path(), "USA");
    fs::write(folder.join("owned.gci"), gci("GKYE01", 2, 1)).unwrap();
    let resolution = resolve_gci(temp.path(), "GKYE01", DolphinCardConfig::AUDITED, None).unwrap();
    let observed = observe(
        &resolution,
        &context(
            Platform::GameCube,
            remote(&gci("GKYE01", 2, 2), "gci", "r1"),
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    let planned = plan_one(observed.input);
    assert_eq!(planned.action, PlanAction::Conflict);
    assert_eq!(planned.reason, ReasonCode::FirstSyncDiverged);
}

// ---------------------------------------------------------------- executor

struct FixedRemote {
    state: RemoteSaveState,
    payload: SavePayload,
    uploads: usize,
}

impl RemoteTransport for FixedRemote {
    fn observe(&mut self, _key: &GameKey) -> Result<RemoteSaveState, CoreError> {
        Ok(self.state.clone())
    }

    fn download(
        &mut self,
        _key: &GameKey,
        _expected: &RemoteSaveSnapshot,
    ) -> Result<SavePayload, CoreError> {
        Ok(self.payload.clone())
    }

    fn upload(
        &mut self,
        key: &GameKey,
        payload: &SavePayload,
        _expected: &ExpectedRemoteState,
    ) -> Result<RemoteSaveSnapshot, CoreError> {
        self.uploads += 1;
        let snapshot = RemoteSaveSnapshot {
            content: payload.content.clone(),
            revision: "uploaded".to_owned(),
            slot: key.slot.clone(),
            extension: "gci".to_owned(),
            size: payload.members[0].bytes.len() as u64,
        };
        self.state = RemoteSaveState::Present(snapshot.clone());
        Ok(snapshot)
    }
}

struct Inactive;

impl ActivityProbe for Inactive {
    fn observe(&self, _key: &GameKey) -> Result<ActivityStatus, CoreError> {
        Ok(ActivityStatus::Inactive)
    }
}

struct Plenty;

impl SpaceProbe for Plenty {
    fn available_bytes(&self, _path: &Path) -> Result<u64, CoreError> {
        Ok(u64::MAX)
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

fn gci_download(
    remote_bytes: &[u8],
) -> (
    TempDir,
    std::path::PathBuf,
    Result<ExecutionOutcome, CoreError>,
) {
    let temp = TempDir::new().unwrap();
    let gc_root = temp.path().join("GC");
    let folder = card_a(&gc_root, "USA");
    let old = gci("GKYE01", 2, 1);
    fs::write(folder.join("owned.gci"), &old).unwrap();
    let resolution = resolve_gci(&gc_root, "GKYE01", DolphinCardConfig::AUDITED, None).unwrap();
    let old_hash = sha256_bytes(&old);
    let head = remote(remote_bytes, "gci", "r2");
    let observed = observe(
        &resolution,
        &context(
            Platform::GameCube,
            head.clone(),
            BaselineState::Known(BaselineSnapshot {
                local_content: old_hash.clone(),
                remote_content: old_hash,
                remote_revision: "r1".to_owned(),
            }),
            SnapshotMode::Qualification,
        ),
    );
    let adapter = observed.input.adapter.clone();
    let planned = plan_one(observed.input);
    assert_eq!(planned.action, PlanAction::Download);
    let mut transport = FixedRemote {
        state: head,
        payload: SavePayload {
            members: vec![PayloadMember {
                name: "owned.gci".to_owned(),
                bytes: remote_bytes.to_vec(),
            }],
            content: sha256_bytes(remote_bytes),
        },
        uploads: 0,
    };
    let store = StateStore::open_in_memory().unwrap();
    let outcome = executor(&temp).execute(
        &store,
        &ExecutionRequest {
            planned,
            mappings: resolution.mappings.clone(),
        },
        &adapter,
        &mut transport,
        &Inactive,
        &Plenty,
        &resolution,
    );
    let live = folder.join("owned.gci");
    (temp, live, outcome)
}

#[test]
fn gci_download_through_guarded_executor_backs_up_and_replaces() {
    let new = gci("GKYE01", 2, 7);
    let (_temp, live, outcome) = gci_download(&new);
    let ExecutionOutcome::Downloaded { backup_path, .. } = outcome.unwrap() else {
        panic!("valid owned GCI download should commit");
    };
    assert_eq!(fs::read(live).unwrap(), new);
    let backup = backup_path.expect("existing GCI is backed up");
    assert_eq!(
        fs::read(backup.join("owned.gci")).unwrap(),
        gci("GKYE01", 2, 1)
    );
}

#[test]
fn foreign_or_malformed_gci_payload_is_rejected_before_staging() {
    let mut truncated = gci("GKYE01", 2, 7);
    truncated.truncate(0x40 + 0x2000);
    for payload in [gci("GALE01", 2, 7), truncated] {
        let (_temp, live, outcome) = gci_download(&payload);
        let ExecutionOutcome::Rejected { code, .. } = outcome.unwrap() else {
            panic!("invalid payload must be rejected");
        };
        assert_eq!(code, RejectionCode::PayloadInvalid);
        assert_eq!(fs::read(&live).unwrap(), gci("GKYE01", 2, 1));
        assert!(
            !live
                .parent()
                .unwrap()
                .join(".romm-autosync-staging")
                .exists()
        );
    }
}

#[test]
fn battery_upload_through_guarded_executor_leaves_local_save_untouched() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("saves");
    fs::create_dir(&root).unwrap();
    let bytes = vec![5; 32768];
    fs::write(root.join("Homebrew.srm"), &bytes).unwrap();
    let resolution = GBA_BATTERY.resolve(&root, "Homebrew", None).unwrap();
    let observed = observe(
        &resolution,
        &context(
            Platform::GameBoyAdvance,
            RemoteSaveState::Absent,
            BaselineState::None,
            SnapshotMode::Qualification,
        ),
    );
    let adapter = observed.input.adapter.clone();
    let planned = plan_one(observed.input);
    let mut transport = FixedRemote {
        state: RemoteSaveState::Absent,
        payload: SavePayload {
            members: Vec::new(),
            content: sha256_bytes(&[]),
        },
        uploads: 0,
    };
    let store = StateStore::open_in_memory().unwrap();
    let outcome = executor(&temp)
        .execute(
            &store,
            &ExecutionRequest {
                planned,
                mappings: resolution.mappings.clone(),
            },
            &adapter,
            &mut transport,
            &Inactive,
            &Plenty,
            &resolution,
        )
        .unwrap();
    assert!(matches!(outcome, ExecutionOutcome::Uploaded { .. }));
    assert_eq!(transport.uploads, 1);
    assert_eq!(fs::read(root.join("Homebrew.srm")).unwrap(), bytes);
}

// ---------------------------------------------------------------- identity

#[test]
fn region_and_revision_mismatches_fail_closed() {
    assert_eq!(
        compare_rom_names("Game (USA).gba", "Game (USA).gba"),
        Ok(())
    );
    assert_eq!(
        compare_rom_names("Game (USA).gba", "Game (Europe).gba"),
        Err(ReadinessReason::RomRegionMismatch)
    );
    assert_eq!(
        compare_rom_names("Game (USA).gba", "Game (USA) (Rev 1).gba"),
        Err(ReadinessReason::RomRevisionMismatch)
    );
    assert_eq!(
        compare_rom_names("Game (USA) (v1.1).nds", "Game (USA) (v1.0).nds"),
        Err(ReadinessReason::RomRevisionMismatch)
    );
    assert_eq!(
        compare_rom_names("Game (USA).gba", "Other Game (USA).gba"),
        Err(ReadinessReason::RomTitleMismatch)
    );
    // Language tags and disc numbers do not change save identity.
    assert_eq!(
        compare_rom_names("Game (Europe) (En,Fr,De).iso", "Game (Europe) (Disc 2).iso"),
        Ok(())
    );

    assert_eq!(
        assess_identity(true, "Game (USA).nds", "Game (USA).nds"),
        (IdentityStatus::Verified, None)
    );
    assert_eq!(
        assess_identity(false, "Game (USA).nds", "Game (USA).nds"),
        (
            IdentityStatus::Ambiguous,
            Some(ReadinessReason::HashIdentityUnverified)
        )
    );
    assert_eq!(
        assess_identity(true, "Game (USA).nds", "Game (Japan).nds").0,
        IdentityStatus::Ambiguous
    );
}

#[test]
fn multi_disc_content_requires_one_consistent_playlist() {
    let parsed = parse_rom_name("Final Game (USA) (Disc 2) (Rev 1).chd");
    assert_eq!(parsed.title, "final game");
    assert_eq!(parsed.regions, ["usa"]);
    assert_eq!(parsed.disc, Some(2));
    assert_eq!(parsed.revision.as_deref(), Some("1"));

    assert_eq!(
        retroarch_content_stem(&["roms/psx/Game (USA).chd"]),
        Ok("Game (USA)".to_owned())
    );
    assert_eq!(
        retroarch_content_stem(&[
            "Game (USA) (Disc 1).chd",
            "Game (USA) (Disc 2).chd",
            "Game (USA).m3u",
        ]),
        Ok("Game (USA)".to_owned())
    );
    assert_eq!(
        retroarch_content_stem(&["Game (USA) (Disc 1).chd", "Game (USA) (Disc 2).chd"]),
        Err(ReadinessReason::MultiDiscWithoutPlaylist)
    );
    assert_eq!(
        retroarch_content_stem(&[
            "Game (USA) (Disc 1).chd",
            "Game (Europe) (Disc 2).chd",
            "Game.m3u",
        ]),
        Err(ReadinessReason::RomRegionMismatch)
    );
    assert_eq!(
        retroarch_content_stem(&["Game (USA).chd", "Other (USA).chd"]),
        Err(ReadinessReason::AmbiguousContent)
    );
    assert_eq!(
        retroarch_content_stem(&[]),
        Err(ReadinessReason::AmbiguousContent)
    );
}

#[test]
fn gamecube_multi_disc_titles_share_one_gci_owner() {
    // Both discs of a two-disc GameCube title report the same six-byte game
    // ID, so one GCI folder resolution covers either disc.
    let temp = TempDir::new().unwrap();
    let folder = card_a(temp.path(), "USA");
    fs::write(folder.join("01-G4BE-save.gci"), gci("G4BE08", 3, 1)).unwrap();
    let disc_one = resolve_gci(temp.path(), "G4BE08", DolphinCardConfig::AUDITED, None).unwrap();
    let disc_two = resolve_gci(temp.path(), "G4BE08", DolphinCardConfig::AUDITED, None).unwrap();
    assert_eq!(disc_one, disc_two);
    assert_eq!(disc_one.files, ["01-G4BE-save.gci"]);
    assert_eq!(
        compare_rom_names("Game (USA) (Disc 1).rvz", "Game (USA) (Disc 2).rvz"),
        Ok(())
    );
}
