use std::{fs, process::Command};

use romm_sync_core::StateStore;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn dry_run_and_status_are_read_only_json_commands() {
    let temp = TempDir::new().expect("temporary directory");
    let save_root = temp.path().join("saves");
    fs::create_dir(&save_root).expect("save root");
    let save_path = save_root.join("game.srm");
    fs::write(&save_path, b"fixture-save").expect("fixture save");
    let manifest_path = temp.path().join("manifest.json");
    let manifest = json!({
        "schema_version": 1,
        "items": [{
            "platform": "nds",
            "rom_id": "fixture-rom",
            "slot": "autosave",
            "adapter": {
                "id": "nds-fixture",
                "revision": "1",
                "status": "validated",
                "save_model": "single_file",
                "compatibility": "compatible",
                "can_upload": true,
                "can_download": true
            },
            "identity": "verified",
            "activity": "inactive",
            "root": save_root,
            "files": ["game.srm"],
            "remote": { "state": "absent" },
            "baseline": { "state": "none" }
        }]
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("manifest file");

    let before = fs::read(&save_path).expect("save before dry-run");
    let output = Command::new(env!("CARGO_BIN_EXE_romm-sync"))
        .arg("dry-run")
        .arg(&manifest_path)
        .output()
        .expect("dry-run command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("dry-run JSON");
    assert_eq!(report["read_only"], true);
    assert_eq!(report["items"][0]["action"], "upload");
    assert_eq!(fs::read(&save_path).expect("save after dry-run"), before);

    let database = temp.path().join("state.db");
    drop(StateStore::open(&database).expect("state database"));
    let before_database = fs::read(&database).expect("database before status");
    let output = Command::new(env!("CARGO_BIN_EXE_romm-sync"))
        .arg("status")
        .arg(&database)
        .output()
        .expect("status command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).expect("status JSON");
    assert_eq!(status["schema_version"], 1);
    assert_eq!(
        fs::read(&database).expect("database after status"),
        before_database
    );
}

#[test]
fn unsafe_manifest_path_is_rejected_before_discovery() {
    let temp = TempDir::new().expect("temporary directory");
    let save_root = temp.path().join("saves");
    fs::create_dir(&save_root).expect("save root");
    let outside = temp.path().join("outside.srm");
    fs::write(&outside, b"must-not-be-read-or-changed").expect("outside fixture");
    let manifest_path = temp.path().join("unsafe.json");
    let manifest = json!({
        "schema_version": 1,
        "items": [{
            "platform": "nds",
            "rom_id": "fixture-rom",
            "slot": "autosave",
            "adapter": {
                "id": "nds-fixture",
                "revision": "1",
                "status": "validated",
                "save_model": "single_file",
                "compatibility": "compatible",
                "can_upload": true,
                "can_download": true
            },
            "identity": "verified",
            "activity": "inactive",
            "root": save_root,
            "files": ["../outside.srm"],
            "remote": { "state": "absent" },
            "baseline": { "state": "none" }
        }]
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("manifest file");

    let output = Command::new(env!("CARGO_BIN_EXE_romm-sync"))
        .arg("dry-run")
        .arg(&manifest_path)
        .output()
        .expect("dry-run command");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsafe relative path"));
    assert_eq!(
        fs::read(outside).expect("outside fixture after rejection"),
        b"must-not-be-read-or-changed"
    );
}
