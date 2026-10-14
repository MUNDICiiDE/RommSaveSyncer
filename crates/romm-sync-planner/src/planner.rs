use std::collections::BTreeMap;

use crate::{
    ActivityRequirement, ActivityStatus, AdapterStatus, BaselineState, CompatibilityStatus,
    ExpectedLocalState, ExpectedRemoteState, FirstSyncPolicy, GameKey, IdentityStatus,
    LocalSaveState, MutationPreconditions, PlanAction, PlannedItem, ReasonCode,
    ReconciliationInput, ReconciliationPlan, RemoteDeletionPolicy, RemoteSaveState, RootStatus,
    SafetyPolicy, SyncState,
};

/// Complete immutable request to the reconciliation planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannerInput {
    pub policy: SafetyPolicy,
    pub items: Vec<ReconciliationInput>,
}

/// Build a deterministic, side-effect-free reconciliation plan.
///
/// Output is ordered by [`GameKey`], independent of discovery order. Duplicate
/// keys collapse to one non-mutating error so ambiguous input can never produce
/// an executable write.
#[must_use]
pub fn plan(input: &PlannerInput) -> ReconciliationPlan {
    let mut grouped = BTreeMap::<&GameKey, Vec<&ReconciliationInput>>::new();
    for item in &input.items {
        grouped.entry(&item.key).or_default().push(item);
    }

    let items = grouped
        .into_iter()
        .map(|(key, candidates)| {
            if candidates.len() == 1 {
                plan_one(candidates[0], input.policy)
            } else {
                decision(
                    key.clone(),
                    SyncState::Error,
                    PlanAction::Error,
                    ReasonCode::DuplicateGameKey,
                )
            }
        })
        .collect();

    ReconciliationPlan { items }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DesiredDecision {
    state: SyncState,
    action: PlanAction,
    reason: ReasonCode,
}

const fn desired(state: SyncState, action: PlanAction, reason: ReasonCode) -> DesiredDecision {
    DesiredDecision {
        state,
        action,
        reason,
    }
}

#[allow(clippy::too_many_lines)]
fn plan_one(input: &ReconciliationInput, policy: SafetyPolicy) -> PlannedItem {
    if input.adapter.platform != input.key.platform {
        return blocked(
            input,
            SyncState::Error,
            PlanAction::Error,
            ReasonCode::AdapterPlatformMismatch,
        );
    }

    match input.adapter.status {
        AdapterStatus::Validated => {}
        AdapterStatus::PendingValidation => {
            return blocked(
                input,
                SyncState::Unsupported,
                PlanAction::Unsupported,
                ReasonCode::AdapterPendingValidation,
            );
        }
        AdapterStatus::Unsupported => {
            return blocked(
                input,
                SyncState::Unsupported,
                PlanAction::Unsupported,
                ReasonCode::AdapterUnsupported,
            );
        }
        AdapterStatus::TemporarilyUnsafe => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::AdapterTemporarilyUnsafe,
            );
        }
    }

    match input.identity {
        IdentityStatus::Verified => {}
        IdentityStatus::Ambiguous => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::IdentityAmbiguous,
            );
        }
        IdentityStatus::Missing => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::IdentityMissing,
            );
        }
    }

    match input.root {
        RootStatus::Available(_) => {}
        RootStatus::Missing => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::RootMissing,
            );
        }
        RootStatus::Unmounted => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::RootUnmounted,
            );
        }
    }

    match input.local {
        LocalSaveState::Absent | LocalSaveState::Present(_) => {}
        LocalSaveState::Incomplete => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::LocalGroupIncomplete,
            );
        }
        LocalSaveState::Malformed => {
            return blocked(
                input,
                SyncState::Error,
                PlanAction::Error,
                ReasonCode::LocalGroupMalformed,
            );
        }
    }

    match input.remote {
        RemoteSaveState::Unavailable => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::RemoteUnavailable,
            );
        }
        RemoteSaveState::Malformed => {
            return blocked(
                input,
                SyncState::Error,
                PlanAction::Error,
                ReasonCode::RemoteMalformed,
            );
        }
        RemoteSaveState::Absent | RemoteSaveState::Present(_) | RemoteSaveState::Deleted { .. } => {
        }
    }

    if input.baseline == BaselineState::Invalid {
        return blocked(
            input,
            SyncState::Error,
            PlanAction::Error,
            ReasonCode::InvalidBaseline,
        );
    }

    match input.adapter.compatibility {
        CompatibilityStatus::Compatible => {}
        CompatibilityStatus::Incompatible => {
            return blocked(
                input,
                SyncState::Unsupported,
                PlanAction::Unsupported,
                ReasonCode::IncompatibleFormat,
            );
        }
        CompatibilityStatus::Unknown => {
            return blocked(
                input,
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::UnknownCompatibility,
            );
        }
    }

    if let RemoteSaveState::Present(remote) = &input.remote {
        if remote.slot != input.key.slot || remote.slot != input.adapter.expected_slot {
            return blocked(
                input,
                SyncState::Error,
                PlanAction::Error,
                ReasonCode::SlotMismatch,
            );
        }
    }

    let desired = reconcile(input, policy);
    if !desired.action.is_mutation() {
        return blocked(input, desired.state, desired.action, desired.reason);
    }

    if desired.action == PlanAction::Upload && !input.adapter.can_upload {
        return blocked(
            input,
            SyncState::Unsupported,
            PlanAction::Unsupported,
            ReasonCode::UploadNotSupported,
        );
    }
    if desired.action == PlanAction::Download && !input.adapter.can_download {
        return blocked(
            input,
            SyncState::Unsupported,
            PlanAction::Unsupported,
            ReasonCode::DownloadNotSupported,
        );
    }

    match policy.activity {
        ActivityRequirement::RequireConfirmedInactive => match input.activity {
            ActivityStatus::Inactive => {}
            ActivityStatus::Active => {
                return blocked(
                    input,
                    SyncState::Pending,
                    PlanAction::Defer,
                    ReasonCode::GameActive,
                );
            }
            ActivityStatus::Unknown => {
                return blocked(
                    input,
                    SyncState::Pending,
                    PlanAction::Defer,
                    ReasonCode::ActivityUnknown,
                );
            }
        },
    }

    PlannedItem {
        key: input.key.clone(),
        state: desired.state,
        action: desired.action,
        reason: desired.reason,
        preconditions: Some(build_preconditions(input, desired.action)),
    }
}

#[allow(clippy::too_many_lines)]
fn reconcile(input: &ReconciliationInput, policy: SafetyPolicy) -> DesiredDecision {
    if matches!(input.remote, RemoteSaveState::Deleted { .. }) {
        return match (&input.local, policy.remote_deletion) {
            (LocalSaveState::Present(_), RemoteDeletionPolicy::PreserveLocalAndDefer) => desired(
                SyncState::Pending,
                PlanAction::Defer,
                ReasonCode::RemoteDeletedPreserveLocal,
            ),
            (LocalSaveState::Absent, RemoteDeletionPolicy::PreserveLocalAndDefer) => desired(
                SyncState::NoSave,
                PlanAction::NoOp,
                ReasonCode::RemoteDeletedNoLocalSave,
            ),
            _ => unreachable!("invalid local states are rejected before reconciliation"),
        };
    }

    match (&input.local, &input.remote, &input.baseline) {
        (LocalSaveState::Present(local), RemoteSaveState::Present(remote), _)
            if local.content == remote.content =>
        {
            desired(
                SyncState::InSync,
                PlanAction::NoOp,
                ReasonCode::ContentMatches,
            )
        }
        (LocalSaveState::Present(_), RemoteSaveState::Present(_), BaselineState::None) => {
            match policy.first_sync {
                FirstSyncPolicy::ConflictOnDivergence => desired(
                    SyncState::Conflict,
                    PlanAction::Conflict,
                    ReasonCode::FirstSyncDiverged,
                ),
            }
        }
        (
            LocalSaveState::Present(local),
            RemoteSaveState::Present(remote),
            BaselineState::Known(baseline),
        ) => {
            let local_changed = local.content != baseline.local_content;
            let remote_changed = remote.content != baseline.remote_content;
            match (local_changed, remote_changed) {
                (false, false) => desired(
                    SyncState::InSync,
                    PlanAction::NoOp,
                    ReasonCode::ContentMatches,
                ),
                (true, false) => desired(
                    SyncState::LocalNewer,
                    PlanAction::Upload,
                    ReasonCode::LocalChanged,
                ),
                (false, true) => desired(
                    SyncState::RemoteNewer,
                    PlanAction::Download,
                    ReasonCode::RemoteChanged,
                ),
                (true, true) => desired(
                    SyncState::Conflict,
                    PlanAction::Conflict,
                    ReasonCode::BothChanged,
                ),
            }
        }
        (LocalSaveState::Present(_), RemoteSaveState::Absent, BaselineState::None) => desired(
            SyncState::LocalOnly,
            PlanAction::Upload,
            ReasonCode::LocalOnlyInitialUpload,
        ),
        (LocalSaveState::Present(_), RemoteSaveState::Absent, BaselineState::Known(_)) => desired(
            SyncState::Pending,
            PlanAction::Defer,
            ReasonCode::RemoteDeletedPreserveLocal,
        ),
        (LocalSaveState::Absent, RemoteSaveState::Present(_), BaselineState::None) => desired(
            SyncState::RemoteOnly,
            PlanAction::Download,
            ReasonCode::RemoteOnlyInitialDownload,
        ),
        (LocalSaveState::Absent, RemoteSaveState::Present(_), BaselineState::Known(_)) => desired(
            SyncState::Conflict,
            PlanAction::Conflict,
            ReasonCode::LocalMissingAfterBaseline,
        ),
        (LocalSaveState::Absent, RemoteSaveState::Absent, BaselineState::None) => desired(
            SyncState::NoSave,
            PlanAction::NoOp,
            ReasonCode::NoSavePresent,
        ),
        (LocalSaveState::Absent, RemoteSaveState::Absent, BaselineState::Known(_)) => desired(
            SyncState::Pending,
            PlanAction::Defer,
            ReasonCode::BothMissingAfterBaseline,
        ),
        _ => unreachable!("invalid states are rejected before reconciliation"),
    }
}

fn build_preconditions(input: &ReconciliationInput, action: PlanAction) -> MutationPreconditions {
    let root = match &input.root {
        RootStatus::Available(root) => root.clone(),
        RootStatus::Missing | RootStatus::Unmounted => {
            unreachable!("root availability is checked before planning mutations")
        }
    };

    let (expected_local, local_size) = match &input.local {
        LocalSaveState::Absent => (ExpectedLocalState::Absent, 0),
        LocalSaveState::Present(group) => {
            let normalized = group.normalized();
            let size = normalized.total_size;
            (
                ExpectedLocalState::Present {
                    content: normalized.content,
                    integrity: normalized.integrity,
                    files: normalized.files,
                },
                size,
            )
        }
        LocalSaveState::Incomplete | LocalSaveState::Malformed => {
            unreachable!("local save validity is checked before planning mutations")
        }
    };

    let (expected_remote, remote_size) = match &input.remote {
        RemoteSaveState::Absent => (ExpectedRemoteState::Absent, 0),
        RemoteSaveState::Present(remote) => (
            ExpectedRemoteState::Present {
                content: remote.content.clone(),
                revision: remote.revision.clone(),
            },
            remote.size,
        ),
        RemoteSaveState::Deleted { .. }
        | RemoteSaveState::Unavailable
        | RemoteSaveState::Malformed => {
            unreachable!("remote save validity is checked before planning mutations")
        }
    };

    MutationPreconditions {
        expected_adapter: input.adapter.clone(),
        root,
        expected_local,
        expected_remote,
        required_activity: ActivityStatus::Inactive,
        minimum_free_bytes: if action == PlanAction::Upload {
            local_size
        } else {
            remote_size
        },
    }
}

fn blocked(
    input: &ReconciliationInput,
    state: SyncState,
    action: PlanAction,
    reason: ReasonCode,
) -> PlannedItem {
    decision(input.key.clone(), state, action, reason)
}

fn decision(key: GameKey, state: SyncState, action: PlanAction, reason: ReasonCode) -> PlannedItem {
    debug_assert!(!action.is_mutation());
    PlannedItem {
        key,
        state,
        action,
        reason,
        preconditions: None,
    }
}
