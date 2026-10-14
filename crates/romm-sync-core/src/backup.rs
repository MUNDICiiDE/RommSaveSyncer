use std::{
    fs,
    path::{Path, PathBuf},
};

use romm_sync_planner::{FileSnapshot, Fingerprint, GameKey, HashAlgorithm};
use serde::{Deserialize, Serialize};

use crate::{
    CoreError, copy_file, io_error, remove_file_if_exists, safe_segment, sha256_bytes, sha256_file,
    validate_relative, write_all_synced,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackupPolicy {
    pub routine_versions: usize,
    pub storage_cap_bytes: Option<u64>,
}

impl Default for BackupPolicy {
    fn default() -> Self {
        Self {
            routine_versions: 10,
            storage_cap_bytes: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupRecord {
    pub directory: PathBuf,
    pub relative_files: Vec<String>,
    pub files: Vec<FileSnapshot>,
    pub total_bytes: u64,
}

pub struct BackupManager {
    root: PathBuf,
    policy: BackupPolicy,
}

#[derive(Deserialize, Serialize)]
struct BackupManifest {
    kind: String,
    platform: String,
    rom_id: String,
    slot: String,
    files: Vec<BackupManifestFile>,
}

#[derive(Deserialize, Serialize)]
struct BackupManifestFile {
    relative_path: String,
    sha256: String,
    size: u64,
}

impl BackupManager {
    #[must_use]
    pub fn new(root: PathBuf, policy: BackupPolicy) -> Self {
        Self { root, policy }
    }

    /// Creates and verifies a routine backup before a live save is changed.
    ///
    /// # Errors
    ///
    /// Returns an error when a source cannot be read, a destination cannot be
    /// written, verification fails, or the retention policy cannot be applied.
    pub fn create_routine_backup(
        &self,
        journal_id: i64,
        key: &GameKey,
        live_root: &Path,
        files: &[FileSnapshot],
    ) -> Result<Option<BackupRecord>, CoreError> {
        if files.is_empty() {
            return Ok(None);
        }
        let directory = self
            .root
            .join("routine")
            .join(game_directory(key))
            .join(format!("{journal_id:020}"));
        let record = self.copy_group("routine", key, live_root, files, directory)?;
        self.enforce_retention(key, &record.directory)?;
        Ok(Some(record))
    }

    /// Creates a protected conflict archive that routine retention never removes.
    ///
    /// # Errors
    ///
    /// Returns an error when the archive already exists, copying fails, or a
    /// copied file does not match the planned size and SHA-256 digest.
    pub fn create_conflict_archive(
        &self,
        archive_id: &str,
        key: &GameKey,
        live_root: &Path,
        files: &[FileSnapshot],
    ) -> Result<BackupRecord, CoreError> {
        let directory = self
            .root
            .join("conflicts")
            .join(game_directory(key))
            .join(safe_segment(archive_id));
        self.copy_group("conflict", key, live_root, files, directory)
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn restore(&self, record: &BackupRecord, live_root: &Path) -> Result<(), CoreError> {
        for file in &record.files {
            validate_relative(&file.relative_path)?;
            let source = record.directory.join(&file.relative_path);
            let metadata =
                fs::metadata(&source).map_err(|error| io_error("metadata", &source, error))?;
            if metadata.len() != file.size || sha256_file(&source)? != file.integrity {
                return Err(CoreError::Rejected(format!(
                    "backup verification failed for {}",
                    file.relative_path
                )));
            }
        }
        for file in &record.files {
            let source = record.directory.join(&file.relative_path);
            let destination = live_root.join(&file.relative_path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io_error("create directory", parent, error))?;
            }
            let temporary = destination.with_extension("romm-rollback.tmp");
            remove_file_if_exists(&temporary)?;
            copy_file(&source, &temporary)?;
            if sha256_file(&temporary)? != file.integrity {
                remove_file_if_exists(&temporary)?;
                return Err(CoreError::Rejected(format!(
                    "rollback staging verification failed for {}",
                    file.relative_path
                )));
            }
            replace_file(&temporary, &destination)?;
        }
        Ok(())
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn load_record(&self, directory: &Path) -> Result<BackupRecord, CoreError> {
        let manifest_path = directory.join("manifest.json");
        let manifest: BackupManifest = serde_json::from_slice(&crate::read_all(&manifest_path)?)?;
        let mut files = Vec::with_capacity(manifest.files.len());
        let mut total_bytes = 0_u64;
        for file in manifest.files {
            validate_relative(&file.relative_path)?;
            total_bytes = total_bytes
                .checked_add(file.size)
                .ok_or_else(|| CoreError::Rejected("backup size exceeds u64".to_owned()))?;
            files.push(FileSnapshot {
                relative_path: file.relative_path,
                integrity: Fingerprint::new(HashAlgorithm::Sha256, file.sha256),
                size: file.size,
            });
        }
        files.sort();
        let relative_files = files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect();
        Ok(BackupRecord {
            directory: directory.to_owned(),
            relative_files,
            files,
            total_bytes,
        })
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn remove_created_files(
        &self,
        live_root: &Path,
        relative_files: &[String],
    ) -> Result<(), CoreError> {
        for relative in relative_files {
            validate_relative(relative)?;
            remove_file_if_exists(&live_root.join(relative))?;
        }
        Ok(())
    }

    #[allow(clippy::unused_self)]
    fn copy_group(
        &self,
        kind: &str,
        key: &GameKey,
        live_root: &Path,
        files: &[FileSnapshot],
        directory: PathBuf,
    ) -> Result<BackupRecord, CoreError> {
        if directory.exists() {
            return Err(CoreError::Rejected(format!(
                "backup directory already exists: {}",
                directory.display()
            )));
        }
        fs::create_dir_all(&directory)
            .map_err(|error| io_error("create backup directory", &directory, error))?;
        let mut relative_files = Vec::with_capacity(files.len());
        let mut total_bytes = 0_u64;
        for file in files {
            validate_relative(&file.relative_path)?;
            let source = live_root.join(&file.relative_path);
            let destination = directory.join(&file.relative_path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io_error("create backup directory", parent, error))?;
            }
            let copied = copy_file(&source, &destination)?;
            let copied_hash = sha256_file(&destination)?;
            if copied != file.size || copied_hash != file.integrity {
                return Err(CoreError::Rejected(format!(
                    "backup verification failed for {}",
                    file.relative_path
                )));
            }
            total_bytes = total_bytes
                .checked_add(copied)
                .ok_or_else(|| CoreError::Rejected("backup size exceeds u64".to_owned()))?;
            relative_files.push(file.relative_path.clone());
        }

        let manifest = BackupManifest {
            kind: kind.to_owned(),
            platform: key.platform.slug().to_owned(),
            rom_id: key.rom_id.clone(),
            slot: key.slot.0.clone(),
            files: files
                .iter()
                .map(|file| BackupManifestFile {
                    relative_path: file.relative_path.clone(),
                    sha256: file.integrity.digest.clone(),
                    size: file.size,
                })
                .collect(),
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        write_all_synced(&directory.join("manifest.json"), &manifest_bytes)?;

        Ok(BackupRecord {
            directory,
            relative_files,
            files: files.to_vec(),
            total_bytes,
        })
    }

    fn enforce_retention(&self, key: &GameKey, protected: &Path) -> Result<(), CoreError> {
        let game_root = self.root.join("routine").join(game_directory(key));
        if !game_root.exists() {
            return Ok(());
        }
        let mut versions = child_directories(&game_root)?;
        versions.sort();
        while versions.len() > self.policy.routine_versions.max(1) {
            let Some(index) = versions.iter().position(|version| version != protected) else {
                break;
            };
            let oldest = versions.remove(index);
            fs::remove_dir_all(&oldest)
                .map_err(|error| io_error("prune backup", &oldest, error))?;
        }

        if let Some(cap) = self.policy.storage_cap_bytes {
            let routine_root = self.root.join("routine");
            let mut all_versions = descendant_version_directories(&routine_root)?;
            all_versions.sort();
            let mut total = directory_size(&routine_root)?;
            for version in all_versions {
                if total <= cap {
                    break;
                }
                if version == protected {
                    continue;
                }
                let size = directory_size(&version)?;
                fs::remove_dir_all(&version)
                    .map_err(|error| io_error("prune backup", &version, error))?;
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }
}

pub(crate) fn replace_file(source: &Path, destination: &Path) -> Result<(), CoreError> {
    #[cfg(unix)]
    {
        fs::rename(source, destination)
            .map_err(|error| io_error("atomic rename", destination, error))
    }
    #[cfg(windows)]
    {
        let old = destination.with_extension("romm-replaced.old");
        remove_file_if_exists(&old)?;
        if destination.exists() {
            fs::rename(destination, &old)
                .map_err(|error| io_error("prepare replacement", destination, error))?;
        }
        match fs::rename(source, destination) {
            Ok(()) => {
                remove_file_if_exists(&old)?;
                Ok(())
            }
            Err(error) => {
                if old.exists() {
                    let _ = fs::rename(&old, destination);
                }
                Err(io_error("replace", destination, error))
            }
        }
    }
}

fn game_directory(key: &GameKey) -> String {
    let raw = format!("{}-{}-{}", key.platform.slug(), key.rom_id, key.slot.0);
    let digest = sha256_bytes(raw.as_bytes()).digest;
    format!("{}-{}", safe_segment(&raw), &digest[..12])
}

fn child_directories(path: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let entries = fs::read_dir(path).map_err(|error| io_error("read directory", path, error))?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io_error("read directory", path, error))?;
        if entry
            .file_type()
            .map_err(|error| io_error("read file type", entry.path(), error))?
            .is_dir()
        {
            directories.push(entry.path());
        }
    }
    Ok(directories)
}

fn descendant_version_directories(root: &Path) -> Result<Vec<PathBuf>, CoreError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut versions = Vec::new();
    for game in child_directories(root)? {
        versions.extend(child_directories(&game)?);
    }
    Ok(versions)
}

fn directory_size(path: &Path) -> Result<u64, CoreError> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path).map_err(|error| io_error("read directory", path, error))? {
        let entry = entry.map_err(|error| io_error("read directory", path, error))?;
        let metadata = entry
            .metadata()
            .map_err(|error| io_error("read metadata", entry.path(), error))?;
        let size = if metadata.is_dir() {
            directory_size(&entry.path())?
        } else {
            metadata.len()
        };
        total = total
            .checked_add(size)
            .ok_or_else(|| CoreError::Rejected("backup size exceeds u64".to_owned()))?;
    }
    Ok(total)
}
