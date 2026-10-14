mod support;

use romm_sync_planner::*;
use support::*;

#[test]
fn first_sync_divergence_is_a_conflict() {
    let mut observed = input(Platform::NintendoDs, "rom-1");
    observed.local = LocalSaveState::Present(group("local-newer-by-clock"));
    observed.remote = RemoteSaveState::Present(remote("remote-older-by-clock", "r9"));
    observed.baseline = BaselineState::None;

    let item = only_item(observed);
    assert_eq!(item.state, SyncState::Conflict);
    assert_eq!(item.action, PlanAction::Conflict);
    assert_eq!(item.reason, ReasonCode::FirstSyncDiverged);
    assert!(item.preconditions.is_none());
}

#[test]
fn simultaneous_changes_are_a_conflict() {
    let mut observed = input(Platform::NintendoDs, "rom-1");
    observed.local = LocalSaveState::Present(group("local-v2"));
    observed.remote = RemoteSaveState::Present(remote("remote-v2", "r2"));
    observed.baseline = BaselineState::Known(baseline("common-v1", "common-v1", "r1"));

    let item = only_item(observed);
    assert_eq!(item.action, PlanAction::Conflict);
    assert_eq!(item.reason, ReasonCode::BothChanged);
}

#[test]
fn remote_deletion_preserves_the_local_save() {
    let mut observed = input(Platform::NintendoDs, "rom-1");
    observed.remote = RemoteSaveState::Deleted {
        last_revision: Some("r1".to_owned()),
    };

    let item = only_item(observed);
    assert_eq!(item.action, PlanAction::Defer);
    assert_eq!(item.reason, ReasonCode::RemoteDeletedPreserveLocal);
    assert!(item.preconditions.is_none());
}

#[test]
fn identical_content_is_a_no_op_without_a_baseline() {
    let mut observed = input(Platform::NintendoDs, "rom-1");
    observed.baseline = BaselineState::None;

    let item = only_item(observed);
    assert_eq!(item.state, SyncState::InSync);
    assert_eq!(item.action, PlanAction::NoOp);
    assert_eq!(item.reason, ReasonCode::ContentMatches);
}

#[test]
fn incomplete_groups_are_deferred() {
    let mut observed = input(Platform::NintendoDs, "rom-1");
    observed.local = LocalSaveState::Incomplete;

    let item = only_item(observed);
    assert_eq!(item.action, PlanAction::Defer);
    assert_eq!(item.reason, ReasonCode::LocalGroupIncomplete);
}

#[test]
fn active_and_unknown_activity_block_mutations() {
    for (activity, reason) in [
        (ActivityStatus::Active, ReasonCode::GameActive),
        (ActivityStatus::Unknown, ReasonCode::ActivityUnknown),
    ] {
        let mut observed = input(Platform::NintendoDs, "rom-1");
        observed.remote = RemoteSaveState::Absent;
        observed.baseline = BaselineState::None;
        observed.activity = activity;

        let item = only_item(observed);
        assert_eq!(item.action, PlanAction::Defer);
        assert_eq!(item.reason, reason);
        assert!(item.preconditions.is_none());
    }
}

#[test]
fn pending_and_unsupported_adapters_never_write() {
    for (status, reason) in [
        (
            AdapterStatus::PendingValidation,
            ReasonCode::AdapterPendingValidation,
        ),
        (AdapterStatus::Unsupported, ReasonCode::AdapterUnsupported),
        (
            AdapterStatus::TemporarilyUnsafe,
            ReasonCode::AdapterTemporarilyUnsafe,
        ),
    ] {
        let mut observed = input(Platform::GameBoyAdvance, "rom-2");
        observed.remote = RemoteSaveState::Absent;
        observed.baseline = BaselineState::None;
        observed.adapter.status = status;

        let item = only_item(observed);
        assert!(!item.action.is_mutation());
        assert_eq!(item.reason, reason);
        assert!(item.preconditions.is_none());
    }
}

#[test]
fn local_change_plans_upload_with_complete_guards() {
    let mut observed = input(Platform::NintendoDs, "rom-1");
    observed.local = LocalSaveState::Present(group("v2"));
    observed.remote = RemoteSaveState::Present(remote("v1", "r1"));
    observed.baseline = BaselineState::Known(baseline("v1", "v1", "r1"));

    let item = only_item(observed);
    assert_eq!(item.action, PlanAction::Upload);
    assert_eq!(item.reason, ReasonCode::LocalChanged);
    let guards = item.preconditions.expect("mutation guards");
    assert_eq!(guards.expected_adapter.adapter_revision, "adapter-v1");
    assert_eq!(guards.expected_adapter.status, AdapterStatus::Validated);
    assert_eq!(guards.required_activity, ActivityStatus::Inactive);
    assert_eq!(guards.minimum_free_bytes, 8_192);
    assert!(matches!(
        guards.expected_local,
        ExpectedLocalState::Present { .. }
    ));
    assert!(matches!(
        guards.expected_remote,
        ExpectedRemoteState::Present { .. }
    ));
}

#[test]
fn remote_change_plans_download_with_complete_guards() {
    let mut observed = input(Platform::GameCube, "rom-3");
    observed.adapter.save_model = SaveModel::GameSpecificMemoryCard;
    observed.local = LocalSaveState::Present(group("v1"));
    observed.remote = RemoteSaveState::Present(remote("v2", "r2"));
    observed.baseline = BaselineState::Known(baseline("v1", "v1", "r1"));

    let item = only_item(observed);
    assert_eq!(item.action, PlanAction::Download);
    assert_eq!(item.reason, ReasonCode::RemoteChanged);
    assert!(item.preconditions.is_some());
}

#[test]
fn all_twelve_platforms_are_representable_but_individually_gated() {
    assert_eq!(Platform::ALL.len(), 12);
    for platform in Platform::ALL {
        let mut observed = input(platform, platform.slug());
        observed.adapter.status = AdapterStatus::PendingValidation;
        observed.remote = RemoteSaveState::Absent;
        observed.baseline = BaselineState::None;

        let item = only_item(observed);
        assert_eq!(item.action, PlanAction::Unsupported);
        assert_eq!(item.reason, ReasonCode::AdapterPendingValidation);
    }
}

#[test]
fn root_and_identity_fail_closed() {
    let cases = [
        (
            IdentityStatus::Ambiguous,
            RootStatus::Available(RootSnapshot {
                logical_path: "a".to_owned(),
                resolved_path: "a".to_owned(),
                mount_identity: "m".to_owned(),
            }),
            ReasonCode::IdentityAmbiguous,
        ),
        (
            IdentityStatus::Verified,
            RootStatus::Missing,
            ReasonCode::RootMissing,
        ),
        (
            IdentityStatus::Verified,
            RootStatus::Unmounted,
            ReasonCode::RootUnmounted,
        ),
    ];

    for (identity, root, reason) in cases {
        let mut observed = input(Platform::NintendoDs, "rom-1");
        observed.identity = identity;
        observed.root = root;
        observed.remote = RemoteSaveState::Absent;
        observed.baseline = BaselineState::None;
        let item = only_item(observed);
        assert_eq!(item.action, PlanAction::Defer);
        assert_eq!(item.reason, reason);
    }
}
