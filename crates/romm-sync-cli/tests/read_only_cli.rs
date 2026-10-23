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

#[test]
fn adapter_readiness_lists_all_platforms_with_nothing_enabled() {
    let output = Command::new(env!("CARGO_BIN_EXE_romm-sync"))
        .arg("adapters")
        .output()
        .expect("adapters command");
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(report["platforms"].as_array().map(Vec::len), Some(12));
    assert_eq!(report["automatic_writes_enabled"], json!([]));
    let ps2 = report["platforms"]
        .as_array()
        .and_then(|platforms| platforms.iter().find(|p| p["platform"] == "ps2"))
        .expect("ps2 row");
    assert!(
        ps2["configurations"]
            .as_array()
            .expect("configurations")
            .iter()
            .any(|c| c["stage"] == "unsupported" && c["reasons"] == json!(["shared_memory_card"]))
    );
}

#[test]
fn inspect_commands_hash_owned_saves_without_writing() {
    let temp = TempDir::new().expect("temporary directory");
    let saves = temp.path().join("saves");
    fs::create_dir(&saves).expect("save root");
    fs::write(saves.join("Game.srm"), vec![1_u8; 8192]).expect("battery save");
    fs::write(saves.join("Game.state"), b"state").expect("save state");
    let output = Command::new(env!("CARGO_BIN_EXE_romm-sync"))
        .args(["inspect-battery", "nds"])
        .arg(&saves)
        .arg("Game")
        .output()
        .expect("inspect-battery command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(report["read_only"], true);
    assert_eq!(report["local_state"], "present");
    assert_eq!(report["members"][0]["relative_path"], "Game.srm");
    assert_eq!(report["diagnostics"], json!(["save_state_excluded"]));
    assert_eq!(fs::read_dir(&saves).expect("listing").count(), 2);

    let folder = temp.path().join("GC").join("USA").join("Card A");
    fs::create_dir_all(&folder).expect("GCI folder");
    let mut gci = vec![0_u8; 0x40 + 0x2000];
    gci[..6].copy_from_slice(b"GKYE01");
    gci[0x38..0x3A].copy_from_slice(&1_u16.to_be_bytes());
    fs::write(folder.join("owned.gci"), &gci).expect("GCI save");
    let output = Command::new(env!("CARGO_BIN_EXE_romm-sync"))
        .arg("inspect-gci")
        .arg(temp.path().join("GC"))
        .arg("GKYE01")
        .output()
        .expect("inspect-gci command");
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(report["configuration_id"], "dolphin-gci-folder-ngc");
    assert_eq!(report["members"][0]["relative_path"], "owned.gci");
    assert_eq!(report["members"][0]["size"], 0x40 + 0x2000);
}
