use std::fmt;

/// The twelve target platforms represented by the specification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Platform {
    NintendoDs,
    GameBoyAdvance,
    GameCube,
    NintendoEntertainmentSystem,
    SuperNintendo,
    Nintendo64,
    Wii,
    Nintendo3Ds,
    PlayStation,
    PlayStation2,
    PlayStationPortable,
    NintendoSwitch,
}

impl Platform {
    pub const ALL: [Self; 12] = [
        Self::NintendoDs,
        Self::GameBoyAdvance,
        Self::GameCube,
        Self::NintendoEntertainmentSystem,
        Self::SuperNintendo,
        Self::Nintendo64,
        Self::Wii,
        Self::Nintendo3Ds,
        Self::PlayStation,
        Self::PlayStation2,
        Self::PlayStationPortable,
        Self::NintendoSwitch,
    ];

    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::NintendoDs => "nds",
            Self::GameBoyAdvance => "gba",
            Self::GameCube => "ngc",
            Self::NintendoEntertainmentSystem => "nes",
            Self::SuperNintendo => "snes",
            Self::Nintendo64 => "n64",
            Self::Wii => "wii",
            Self::Nintendo3Ds => "3ds",
            Self::PlayStation => "psx",
            Self::PlayStation2 => "ps2",
            Self::PlayStationPortable => "psp",
            Self::NintendoSwitch => "switch",
        }
    }
}

/// Stable identity for one `RomM` game and save slot.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GameKey {
    pub platform: Platform,
    pub rom_id: String,
    pub slot: SlotId,
}

/// A normalized save slot such as `autosave`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SlotId(pub String);

impl From<&str> for SlotId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// A content or integrity hash with an explicit algorithm.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fingerprint {
    pub algorithm: HashAlgorithm,
    pub digest: String,
}

impl Fingerprint {
    #[must_use]
    pub fn new(algorithm: HashAlgorithm, digest: impl Into<String>) -> Self {
        Self {
            algorithm,
            digest: digest.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HashAlgorithm {
    Crc32,
    Md5,
    Sha1,
    Sha256,
    Other(String),
}

/// The adapter's storage model for a save.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveModel {
    SingleFile,
    MultiFile,
    GameSpecificMemoryCard,
    DirectoryTree,
    SharedMemoryCard,
    Unknown,
}

/// Validation state for an adapter. Only `Validated` adapters may mutate data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterStatus {
    Validated,
    PendingValidation,
    Unsupported,
    TemporarilyUnsafe,
}

/// Format compatibility between the local save representation and `RomM`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityStatus {
    Compatible,
    Incompatible,
    Unknown,
}

/// Immutable adapter facts supplied to the planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterSnapshot {
    pub adapter_id: String,
    pub adapter_revision: String,
    pub platform: Platform,
    pub status: AdapterStatus,
    pub save_model: SaveModel,
    pub compatibility: CompatibilityStatus,
    pub can_upload: bool,
    pub can_download: bool,
    pub expected_slot: SlotId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityStatus {
    Verified,
    Ambiguous,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityStatus {
    Inactive,
    Active,
    Unknown,
}

/// Root and mount identity captured during discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootSnapshot {
    pub logical_path: String,
    pub resolved_path: String,
    pub mount_identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RootStatus {
    Available(RootSnapshot),
    Missing,
    Unmounted,
}

/// One member of a save group. Paths are normalized relative to the save root.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FileSnapshot {
    pub relative_path: String,
    pub integrity: Fingerprint,
    pub size: u64,
}

/// A complete, immutable local save group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveGroupSnapshot {
    /// Comparable logical content hash used for reconciliation.
    pub content: Fingerprint,
    /// Hash over the full normalized group used for stale-input detection.
    pub integrity: Fingerprint,
    pub files: Vec<FileSnapshot>,
    pub total_size: u64,
}

impl SaveGroupSnapshot {
    /// Returns a canonical copy with files sorted by stable member identity.
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.files.sort();
        normalized
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalSaveState {
    Absent,
    Present(SaveGroupSnapshot),
    Incomplete,
    Malformed,
}

/// Immutable `RomM` save metadata needed by reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteSaveSnapshot {
    pub content: Fingerprint,
    pub revision: String,
    pub slot: SlotId,
    pub extension: String,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemoteSaveState {
    Absent,
    Present(RemoteSaveSnapshot),
    /// `RomM` reports a deletion/tombstone. It is never propagated automatically.
    Deleted {
        last_revision: Option<String>,
    },
    Unavailable,
    Malformed,
}

/// The last common local and remote content observed after a successful sync.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaselineSnapshot {
    pub local_content: Fingerprint,
    pub remote_content: Fingerprint,
    pub remote_revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BaselineState {
    None,
    Known(BaselineSnapshot),
    Invalid,
}

/// One fully normalized reconciliation input. The planner borrows and never mutates it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationInput {
    pub key: GameKey,
    pub adapter: AdapterSnapshot,
    pub identity: IdentityStatus,
    pub activity: ActivityStatus,
    pub root: RootStatus,
    pub local: LocalSaveState,
    pub remote: RemoteSaveState,
    pub baseline: BaselineState,
}

/// Stable state classification exposed to UI, logs, and history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncState {
    InSync,
    LocalNewer,
    RemoteNewer,
    LocalOnly,
    RemoteOnly,
    Conflict,
    Pending,
    Unsupported,
    Error,
    NoSave,
}

/// The executor-facing result. Only upload and download mutate save data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanAction {
    Upload,
    Download,
    NoOp,
    Conflict,
    Defer,
    Unsupported,
    Error,
}

impl PlanAction {
    #[must_use]
    pub const fn is_mutation(self) -> bool {
        matches!(self, Self::Upload | Self::Download)
    }
}

/// Machine-stable explanation for a planned action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasonCode {
    ContentMatches,
    NoSavePresent,
    LocalOnlyInitialUpload,
    RemoteOnlyInitialDownload,
    LocalChanged,
    RemoteChanged,
    BothChanged,
    FirstSyncDiverged,
    LocalMissingAfterBaseline,
    BothMissingAfterBaseline,
    RemoteDeletedPreserveLocal,
    RemoteDeletedNoLocalSave,
    AdapterPendingValidation,
    AdapterUnsupported,
    AdapterTemporarilyUnsafe,
    AdapterPlatformMismatch,
    IdentityAmbiguous,
    IdentityMissing,
    GameActive,
    ActivityUnknown,
    RootMissing,
    RootUnmounted,
    LocalGroupIncomplete,
    LocalGroupMalformed,
    RemoteUnavailable,
    RemoteMalformed,
    IncompatibleFormat,
    UnknownCompatibility,
    SlotMismatch,
    UploadNotSupported,
    DownloadNotSupported,
    InvalidBaseline,
    DuplicateGameKey,
}

impl ReasonCode {
    /// Stable wire/log representation. These values are part of the M1A contract.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContentMatches => "content_matches",
            Self::NoSavePresent => "no_save_present",
            Self::LocalOnlyInitialUpload => "local_only_initial_upload",
            Self::RemoteOnlyInitialDownload => "remote_only_initial_download",
            Self::LocalChanged => "local_changed",
            Self::RemoteChanged => "remote_changed",
            Self::BothChanged => "both_changed",
            Self::FirstSyncDiverged => "first_sync_diverged",
            Self::LocalMissingAfterBaseline => "local_missing_after_baseline",
            Self::BothMissingAfterBaseline => "both_missing_after_baseline",
            Self::RemoteDeletedPreserveLocal => "remote_deleted_preserve_local",
            Self::RemoteDeletedNoLocalSave => "remote_deleted_no_local_save",
            Self::AdapterPendingValidation => "adapter_pending_validation",
            Self::AdapterUnsupported => "adapter_unsupported",
            Self::AdapterTemporarilyUnsafe => "adapter_temporarily_unsafe",
            Self::AdapterPlatformMismatch => "adapter_platform_mismatch",
            Self::IdentityAmbiguous => "identity_ambiguous",
            Self::IdentityMissing => "identity_missing",
            Self::GameActive => "game_active",
            Self::ActivityUnknown => "activity_unknown",
            Self::RootMissing => "root_missing",
            Self::RootUnmounted => "root_unmounted",
            Self::LocalGroupIncomplete => "local_group_incomplete",
            Self::LocalGroupMalformed => "local_group_malformed",
            Self::RemoteUnavailable => "remote_unavailable",
            Self::RemoteMalformed => "remote_malformed",
            Self::IncompatibleFormat => "incompatible_format",
            Self::UnknownCompatibility => "unknown_compatibility",
            Self::SlotMismatch => "slot_mismatch",
            Self::UploadNotSupported => "upload_not_supported",
            Self::DownloadNotSupported => "download_not_supported",
            Self::InvalidBaseline => "invalid_baseline",
            Self::DuplicateGameKey => "duplicate_game_key",
        }
    }
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Expected local state that M1B must revalidate immediately before mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpectedLocalState {
    Absent,
    Present {
        content: Fingerprint,
        integrity: Fingerprint,
        files: Vec<FileSnapshot>,
    },
}

/// Expected `RomM` state that M1B must revalidate immediately before mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpectedRemoteState {
    Absent,
    Present {
        content: Fingerprint,
        revision: String,
    },
}

/// Complete stale-observation guard attached to every mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationPreconditions {
    pub expected_adapter: AdapterSnapshot,
    pub root: RootSnapshot,
    pub expected_local: ExpectedLocalState,
    pub expected_remote: ExpectedRemoteState,
    pub required_activity: ActivityStatus,
    pub minimum_free_bytes: u64,
}

/// One ordered planner decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedItem {
    pub key: GameKey,
    pub state: SyncState,
    pub action: PlanAction,
    pub reason: ReasonCode,
    pub preconditions: Option<MutationPreconditions>,
}

/// Deterministically ordered plan.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReconciliationPlan {
    pub items: Vec<PlannedItem>,
}
