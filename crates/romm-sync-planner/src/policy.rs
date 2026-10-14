/// First-sync policy. Divergent local and remote content is always a conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstSyncPolicy {
    ConflictOnDivergence,
}

/// Remote deletion policy. M1A never plans deletion of local save data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteDeletionPolicy {
    PreserveLocalAndDefer,
}

/// Activity safety policy. Only a confirmed inactive game permits mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityRequirement {
    RequireConfirmedInactive,
}

/// Explicit immutable safety policy supplied to every planner invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyPolicy {
    pub first_sync: FirstSyncPolicy,
    pub remote_deletion: RemoteDeletionPolicy,
    pub activity: ActivityRequirement,
}

impl SafetyPolicy {
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            first_sync: FirstSyncPolicy::ConflictOnDivergence,
            remote_deletion: RemoteDeletionPolicy::PreserveLocalAndDefer,
            activity: ActivityRequirement::RequireConfirmedInactive,
        }
    }
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        Self::strict()
    }
}
