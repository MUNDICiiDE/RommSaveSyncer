use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
    thread,
    time::Duration,
};

use romm_sync_planner::{
    FileSnapshot, GameKey, LocalSaveState, PlanAction, PlannerInput, ReconciliationInput,
    RootSnapshot, RootStatus, SafetyPolicy, SaveGroupSnapshot, SyncState, plan,
};
use serde::Serialize;

use crate::{CoreError, DiscoverySpec, io_error, sha256_file, sha256_group};

pub const DRY_RUN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredItem {
    pub input: ReconciliationInput,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DryRunReport {
    pub schema_version: u32,
    pub read_only: bool,
    pub items: Vec<DryRunItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DryRunItem {
    pub platform: String,
    pub rom_id: String,
    pub slot: String,
    pub state: String,
    pub action: String,
    pub reason: String,
    pub executable: bool,
    pub revalidation_preconditions: bool,
    pub diagnostics: Vec<String>,
}

/// Resolves roots and performs a stable, read-only scan of every save group.
///
/// # Errors
///
/// Returns an error for unsafe relative paths or when a discovered file cannot
/// be read or hashed.
pub fn discover(
    specs: &[DiscoverySpec],
    stability_delay: Duration,
) -> Result<Vec<DiscoveredItem>, CoreError> {
    specs
        .iter()
        .map(|spec| discover_one(spec, stability_delay))
        .collect()
}

/// Builds a deterministic planner report without changing local or remote state.
///
/// # Errors
///
/// Returns an error when discovery fails or the planner rejects malformed input.
pub fn dry_run(
    specs: &[DiscoverySpec],
    stability_delay: Duration,
) -> Result<DryRunReport, CoreError> {
    let discovered = discover(specs, stability_delay)?;
    let diagnostics: BTreeMap<_, _> = discovered
        .iter()
        .map(|item| (item.input.key.clone(), item.diagnostics.clone()))
        .collect();
    let planner_input = PlannerInput {
        policy: SafetyPolicy::strict(),
        items: discovered.into_iter().map(|item| item.input).collect(),
    };
    let planned = plan(&planner_input);
    let items = planned
        .items
        .into_iter()
        .map(|item| DryRunItem {
            platform: item.key.platform.slug().to_owned(),
            rom_id: item.key.rom_id.clone(),
            slot: item.key.slot.0.clone(),
            state: state_name(item.state).to_owned(),
            action: action_name(item.action).to_owned(),
            reason: item.reason.as_str().to_owned(),
            executable: item.action.is_mutation(),
            revalidation_preconditions: item.preconditions.is_some(),
            diagnostics: diagnostics.get(&item.key).cloned().unwrap_or_default(),
        })
        .collect();
    Ok(DryRunReport {
        schema_version: DRY_RUN_SCHEMA_VERSION,
        read_only: true,
        items,
    })
}

#[must_use]
pub fn observe_root(path: &Path) -> RootStatus {
    if !path.exists() {
        return RootStatus::Missing;
    }
    let Ok(resolved) = path.canonicalize() else {
        return RootStatus::Unmounted;
    };
    let Ok(mount_identity) = mount_identity(&resolved) else {
        return RootStatus::Unmounted;
    };
    RootStatus::Available(RootSnapshot {
        logical_path: path.to_string_lossy().into_owned(),
        resolved_path: resolved.to_string_lossy().into_owned(),
        mount_identity,
    })
}

/// Captures a complete, hashed save group under an already resolved root.
///
/// # Errors
///
/// Returns an error for unsafe paths or when file metadata or contents cannot
/// be read consistently.
pub fn observe_local_group(
    root: &RootSnapshot,
    relative_files: &[String],
) -> Result<LocalSaveState, CoreError> {
    if relative_files.is_empty() {
        return Ok(LocalSaveState::Malformed);
    }
    let resolved_root = Path::new(&root.resolved_path);
    let mut snapshots = Vec::with_capacity(relative_files.len());
    let mut missing = 0_usize;

    for relative in relative_files {
        validate_relative(relative)?;
        let path = resolved_root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing += 1;
                continue;
            }
            Err(error) => return Err(io_error("metadata", path, error)),
        };
        if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
            return Ok(LocalSaveState::Malformed);
        }
        let canonical = path
            .canonicalize()
            .map_err(|error| io_error("canonicalize", &path, error))?;
        if !canonical.starts_with(resolved_root) {
            return Err(CoreError::UnsafePath(relative.clone()));
        }
        let before =
            fs::metadata(&canonical).map_err(|error| io_error("metadata", &canonical, error))?;
        if !before.is_file() {
            return Ok(LocalSaveState::Malformed);
        }
        let integrity = sha256_file(&canonical)?;
        let after =
            fs::metadata(&canonical).map_err(|error| io_error("metadata", &canonical, error))?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            return Ok(LocalSaveState::Incomplete);
        }
        snapshots.push(FileSnapshot {
            relative_path: normalize_relative(relative),
            integrity,
            size: after.len(),
        });
    }

    if missing == relative_files.len() {
        return Ok(LocalSaveState::Absent);
    }
    if missing > 0 {
        return Ok(LocalSaveState::Incomplete);
    }

    snapshots.sort();
    let total_size = snapshots.iter().try_fold(0_u64, |total, file| {
        total
            .checked_add(file.size)
            .ok_or_else(|| CoreError::InvalidManifest("save group size exceeds u64".to_owned()))
    })?;
    let content = sha256_group(resolved_root, &snapshots)?;
    Ok(LocalSaveState::Present(SaveGroupSnapshot {
        content: content.clone(),
        integrity: content,
        files: snapshots,
        total_size,
    }))
}

/// Rejects absolute paths, traversal, and empty relative paths.
///
/// # Errors
///
/// Returns [`CoreError::UnsafePath`] when `value` can escape the save root.
pub fn validate_relative(value: &str) -> Result<(), CoreError> {
    let path = Path::new(value);
    if value.is_empty() || path.is_absolute() {
        return Err(CoreError::UnsafePath(value.to_owned()));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(CoreError::UnsafePath(value.to_owned()));
    }
    Ok(())
}

fn discover_one(spec: &DiscoverySpec, delay: Duration) -> Result<DiscoveredItem, CoreError> {
    let root = observe_root(&spec.root);
    let mut diagnostics = Vec::new();
    let local = match &root {
        RootStatus::Available(snapshot) => {
            let first = observe_local_group(snapshot, &spec.files)?;
            if !delay.is_zero() && matches!(first, LocalSaveState::Present(_)) {
                thread::sleep(delay);
            }
            let second = observe_local_group(snapshot, &spec.files)?;
            if first == second {
                second
            } else {
                diagnostics.push("files_changed_during_stability_check".to_owned());
                LocalSaveState::Incomplete
            }
        }
        RootStatus::Missing | RootStatus::Unmounted => LocalSaveState::Absent,
    };

    Ok(DiscoveredItem {
        input: ReconciliationInput {
            key: spec.key.clone(),
            adapter: spec.adapter.clone(),
            identity: spec.identity,
            activity: spec.activity,
            root,
            local,
            remote: spec.remote.clone(),
            baseline: spec.baseline.clone(),
        },
        diagnostics,
    })
}

#[cfg(unix)]
fn mount_identity(path: &Path) -> Result<String, CoreError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path).map_err(|error| io_error("metadata", path, error))?;
    Ok(format!("unix-device:{}", metadata.dev()))
}

#[cfg(windows)]
fn mount_identity(path: &Path) -> Result<String, CoreError> {
    let prefix = path.components().next().ok_or_else(|| {
        CoreError::InvalidManifest("resolved root has no volume prefix".to_owned())
    })?;
    Ok(format!("windows-volume:{prefix:?}"))
}

fn normalize_relative(value: &str) -> String {
    value.replace('\\', "/")
}

const fn action_name(action: PlanAction) -> &'static str {
    match action {
        PlanAction::Upload => "upload",
        PlanAction::Download => "download",
        PlanAction::NoOp => "no_op",
        PlanAction::Conflict => "conflict",
        PlanAction::Defer => "defer",
        PlanAction::Unsupported => "unsupported",
        PlanAction::Error => "error",
    }
}

const fn state_name(state: SyncState) -> &'static str {
    match state {
        SyncState::InSync => "in_sync",
        SyncState::LocalNewer => "local_newer",
        SyncState::RemoteNewer => "remote_newer",
        SyncState::LocalOnly => "local_only",
        SyncState::RemoteOnly => "remote_only",
        SyncState::Conflict => "conflict",
        SyncState::Pending => "pending",
        SyncState::Unsupported => "unsupported",
        SyncState::Error => "error",
        SyncState::NoSave => "no_save",
    }
}

pub(crate) fn key_slug(key: &GameKey) -> String {
    format!("{}-{}-{}", key.platform.slug(), key.rom_id, key.slot.0)
}

pub(crate) fn safe_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
