mod support;

use romm_sync_planner::*;
use support::*;

#[test]
fn input_order_does_not_change_the_plan() {
    let mut ds = input(Platform::NintendoDs, "rom-ds");
    ds.remote = RemoteSaveState::Absent;
    ds.baseline = BaselineState::None;

    let mut gba = input(Platform::GameBoyAdvance, "rom-gba");
    gba.local = LocalSaveState::Absent;
    gba.baseline = BaselineState::None;

    let mut gc = input(Platform::GameCube, "rom-gc");
    gc.local = LocalSaveState::Present(group("local-v2"));
    gc.remote = RemoteSaveState::Present(remote("remote-v2", "r2"));
    gc.baseline = BaselineState::Known(baseline("v1", "v1", "r1"));

    let candidates = [ds, gba, gc];
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];

    let expected = plan(&PlannerInput {
        policy: SafetyPolicy::strict(),
        items: candidates.to_vec(),
    });

    for permutation in permutations {
        let request = PlannerInput {
            policy: SafetyPolicy::strict(),
            items: permutation
                .into_iter()
                .map(|index| candidates[index].clone())
                .collect(),
        };
        let before = request.clone();
        let actual = plan(&request);
        assert_eq!(request, before, "planner mutated its input");
        assert_eq!(actual, expected, "discovery order affected output");
    }
}

#[test]
fn file_order_is_canonical_in_mutation_preconditions() {
    let mut observed = input(Platform::NintendoDs, "rom-1");
    let mut changed = group("v2");
    changed.files = vec![
        FileSnapshot {
            relative_path: "z.dat".to_owned(),
            integrity: fingerprint("z"),
            size: 2,
        },
        FileSnapshot {
            relative_path: "a.dat".to_owned(),
            integrity: fingerprint("a"),
            size: 1,
        },
    ];
    changed.total_size = 3;
    observed.local = LocalSaveState::Present(changed);
    observed.remote = RemoteSaveState::Present(remote("v1", "r1"));
    observed.baseline = BaselineState::Known(baseline("v1", "v1", "r1"));

    let item = only_item(observed);
    let ExpectedLocalState::Present { files, .. } =
        item.preconditions.expect("upload guards").expected_local
    else {
        panic!("expected present local state");
    };
    let paths: Vec<_> = files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    assert_eq!(paths, ["a.dat", "z.dat"]);
}

#[test]
fn duplicate_keys_collapse_to_one_non_mutating_error() {
    let first = input(Platform::NintendoDs, "duplicate");
    let mut second = first.clone();
    second.local = LocalSaveState::Present(group("different"));

    let output = plan(&PlannerInput {
        policy: SafetyPolicy::strict(),
        items: vec![second, first],
    });

    assert_eq!(output.items.len(), 1);
    assert_eq!(output.items[0].action, PlanAction::Error);
    assert_eq!(output.items[0].reason, ReasonCode::DuplicateGameKey);
    assert!(output.items[0].preconditions.is_none());
}

#[test]
fn every_planned_mutation_has_revalidation_preconditions() {
    let adapter_statuses = [
        AdapterStatus::Validated,
        AdapterStatus::PendingValidation,
        AdapterStatus::Unsupported,
        AdapterStatus::TemporarilyUnsafe,
    ];
    let identities = [
        IdentityStatus::Verified,
        IdentityStatus::Ambiguous,
        IdentityStatus::Missing,
    ];
    let activities = [
        ActivityStatus::Inactive,
        ActivityStatus::Active,
        ActivityStatus::Unknown,
    ];
    let roots = [
        input(Platform::NintendoDs, "seed").root,
        RootStatus::Missing,
        RootStatus::Unmounted,
    ];
    let locals = [
        LocalSaveState::Absent,
        LocalSaveState::Present(group("local")),
        LocalSaveState::Incomplete,
        LocalSaveState::Malformed,
    ];
    let remotes = [
        RemoteSaveState::Absent,
        RemoteSaveState::Present(remote("remote", "r2")),
        RemoteSaveState::Deleted {
            last_revision: Some("r1".to_owned()),
        },
        RemoteSaveState::Unavailable,
        RemoteSaveState::Malformed,
    ];
    let baselines = [
        BaselineState::None,
        BaselineState::Known(baseline("base", "base", "r1")),
        BaselineState::Invalid,
    ];

    for status in adapter_statuses {
        for identity in identities {
            for activity in activities {
                for root in &roots {
                    for local in &locals {
                        for remote_state in &remotes {
                            for baseline_state in &baselines {
                                let mut observed = input(Platform::NintendoDs, "rom-1");
                                observed.adapter.status = status;
                                observed.identity = identity;
                                observed.activity = activity;
                                observed.root = root.clone();
                                observed.local = local.clone();
                                observed.remote = remote_state.clone();
                                observed.baseline = baseline_state.clone();

                                let item = only_item(observed);
                                assert_eq!(
                                    item.action.is_mutation(),
                                    item.preconditions.is_some(),
                                    "mutation/precondition invariant failed for {item:?}"
                                );
                                if item.action.is_mutation() {
                                    let guards = item.preconditions.expect("mutation guards");
                                    assert_eq!(guards.required_activity, ActivityStatus::Inactive);
                                    assert!(!guards.expected_adapter.adapter_id.is_empty());
                                    assert!(!guards.expected_adapter.adapter_revision.is_empty());
                                    assert_eq!(
                                        guards.expected_adapter.status,
                                        AdapterStatus::Validated
                                    );
                                    assert!(!guards.root.mount_identity.is_empty());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
