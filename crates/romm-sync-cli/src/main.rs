use std::{env, path::Path, process::ExitCode, time::Duration};

use romm_sync_core::{StateStore, dry_run, load_discovery_manifest};

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

fn print_help() {
    println!(
        "RomM Save Syncer read-only CLI\n\n\
         Usage:\n\
           romm-sync dry-run <discovery-manifest.json>\n\
           romm-sync status <state.db>\n\n\
         Both commands are read-only. The dry-run command hashes only the files\n\
         explicitly named by the manifest and never executes its proposed plan."
    );
}
