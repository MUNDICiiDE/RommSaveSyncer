use std::{env, path::Path, process::ExitCode, time::Duration};

use romm_sync_adapters::{
    AdapterResolution, DolphinCardConfig, ObservationContext, SnapshotMode, battery_profile,
    readiness_report, resolve_gci,
};
use romm_sync_core::{StateStore, dry_run, load_discovery_manifest, parse_platform};
use romm_sync_planner::{
    ActivityStatus, BaselineState, GameKey, IdentityStatus, RemoteSaveState, SlotId,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("romm-sync: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    match arguments.as_slice() {
        [command, manifest] if command == "dry-run" => {
            let specs = load_discovery_manifest(Path::new(manifest))?;
            let report = dry_run(&specs, Duration::from_millis(250))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        [command, database] if command == "status" => {
            let store = StateStore::open_read_only(Path::new(database))?;
            println!("{}", serde_json::to_string_pretty(&store.status()?)?);
            Ok(())
        }
        [command] if command == "adapters" => {
            println!("{}", serde_json::to_string_pretty(&readiness_report())?);
            Ok(())
        }
        [command, platform, root, stem] if command == "inspect-battery" => {
            let platform = parse_platform(platform)?;
            let profile = battery_profile(platform)
                .ok_or("platform has no RetroArch battery-save adapter")?;
            inspect(&profile.resolve(Path::new(root), stem, None)?)
        }
        [command, gc_root, game_id] if command == "inspect-gci" => inspect(&resolve_gci(
            Path::new(gc_root),
            game_id,
            DolphinCardConfig::AUDITED,
            None,
        )?),
        [command] if command == "--help" || command == "-h" || command == "help" => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err("invalid arguments".into())
        }
    }
}

/// Resolves and hashes one save without remote, baseline, or activity evidence.
fn inspect(resolution: &AdapterResolution) -> Result<(), Box<dyn std::error::Error>> {
    let observed = resolution.observe(&ObservationContext {
        key: GameKey {
            platform: resolution.definition.platform,
            rom_id: "inspection".to_owned(),
            slot: SlotId::from("autosave"),
        },
        identity: IdentityStatus::Missing,
        activity: ActivityStatus::Unknown,
        remote: RemoteSaveState::Unavailable,
        baseline: BaselineState::None,
        stability_delay: Duration::from_millis(250),
        mode: SnapshotMode::Production,
    })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&resolution.summary(&observed))?
    );
    Ok(())
}

fn print_help() {
    println!(
        "RomM Save Syncer read-only CLI\n\n\
         Usage:\n\
           romm-sync dry-run <discovery-manifest.json>\n\
           romm-sync status <state.db>\n\
           romm-sync adapters\n\
           romm-sync inspect-battery <nds|gba|nes|snes> <save-root> <content-stem>\n\
           romm-sync inspect-gci <dolphin-gc-root> <game-id>\n\n\
         Every command is read-only. The dry-run command hashes only the files\n\
         explicitly named by the manifest and never executes its proposed plan.\n\
         `adapters` prints per-platform readiness and blocking reasons. The\n\
         inspect commands resolve one save through its M3 adapter, hash the owned\n\
         files and report diagnostics without contacting RomM."
    );
}
