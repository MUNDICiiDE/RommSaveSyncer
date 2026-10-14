#![allow(dead_code)]

use romm_sync_planner::*;

pub fn fingerprint(value: &str) -> Fingerprint {
    Fingerprint::new(HashAlgorithm::Sha256, value)
}

pub fn group(value: &str) -> SaveGroupSnapshot {
    SaveGroupSnapshot {
        content: fingerprint(value),
        integrity: fingerprint(&format!("integrity-{value}")),
        files: vec![FileSnapshot {
            relative_path: "save.srm".to_owned(),
            integrity: fingerprint(&format!("file-{value}")),
            size: 8_192,
        }],
        total_size: 8_192,
    }
}

pub fn remote(value: &str, revision: &str) -> RemoteSaveSnapshot {
    RemoteSaveSnapshot {
        content: fingerprint(value),
        revision: revision.to_owned(),
        slot: SlotId::from("autosave"),
        extension: ".sav".to_owned(),
        size: 8_192,
    }
}

pub fn baseline(local: &str, remote: &str, revision: &str) -> BaselineSnapshot {
    BaselineSnapshot {
        local_content: fingerprint(local),
        remote_content: fingerprint(remote),
        remote_revision: revision.to_owned(),
    }
}

pub fn adapter(platform: Platform) -> AdapterSnapshot {
    AdapterSnapshot {
        adapter_id: format!("{}-test", platform.slug()),
        adapter_revision: "adapter-v1".to_owned(),
        platform,
        status: AdapterStatus::Validated,
        save_model: SaveModel::SingleFile,
        compatibility: CompatibilityStatus::Compatible,
        can_upload: true,
        can_download: true,
        expected_slot: SlotId::from("autosave"),
    }
}

pub fn input(platform: Platform, rom_id: &str) -> ReconciliationInput {
    ReconciliationInput {
        key: GameKey {
            platform,
            rom_id: rom_id.to_owned(),
            slot: SlotId::from("autosave"),
        },
        adapter: adapter(platform),
        identity: IdentityStatus::Verified,
        activity: ActivityStatus::Inactive,
        root: RootStatus::Available(RootSnapshot {
            logical_path: "/home/user/Emulation/saves".to_owned(),
            resolved_path: "/var/home/user/Emulation/saves".to_owned(),
            mount_identity: "btrfs:subvol-257".to_owned(),
        }),
        local: LocalSaveState::Present(group("same")),
        remote: RemoteSaveState::Present(remote("same", "remote-r1")),
        baseline: BaselineState::Known(baseline("same", "same", "remote-r1")),
    }
}

pub fn only_item(input: ReconciliationInput) -> PlannedItem {
    let output = plan(&PlannerInput {
        policy: SafetyPolicy::strict(),
        items: vec![input],
    });
    assert_eq!(output.items.len(), 1);
    output.items.into_iter().next().expect("one planned item")
}
