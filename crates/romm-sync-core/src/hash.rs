use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use romm_sync_planner::{FileSnapshot, Fingerprint, HashAlgorithm};
use sha2::{Digest, Sha256};

use crate::{CoreError, io_error};

#[must_use]
pub fn sha256_bytes(bytes: &[u8]) -> Fingerprint {
    let digest = Sha256::digest(bytes);
    Fingerprint::new(HashAlgorithm::Sha256, hex(&digest))
}

/// Hashes a file with SHA-256.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or read.
pub fn sha256_file(path: &Path) -> Result<Fingerprint, CoreError> {
    let mut file = File::open(path).map_err(|error| io_error("open", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error("read", path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Fingerprint::new(
        HashAlgorithm::Sha256,
        hex(&hasher.finalize()),
    ))
}

/// Hash a normalized group. A one-file group hashes only its bytes so an
/// extension-only adapter mapping (`.srm` ↔ `.sav`) remains comparable.
/// Hashes a canonical save group, including member names for multi-file groups.
///
/// # Errors
///
/// Returns an error for unsafe member paths or unreadable files.
pub fn sha256_group(root: &Path, files: &[FileSnapshot]) -> Result<Fingerprint, CoreError> {
    if files.len() == 1 {
        return sha256_file(&root.join(&files[0].relative_path));
    }

    let mut ordered = files.to_vec();
    ordered.sort();
    let mut hasher = Sha256::new();
    for file in ordered {
        let path_bytes = file.relative_path.as_bytes();
        hasher.update((path_bytes.len() as u64).to_le_bytes());
        hasher.update(path_bytes);
        hasher.update(file.size.to_le_bytes());
        hasher.update(file.integrity.digest.as_bytes());
    }
    Ok(Fingerprint::new(
        HashAlgorithm::Sha256,
        hex(&hasher.finalize()),
    ))
}

pub(crate) fn write_all_synced(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    use std::io::Write;

    let mut file = File::create(path).map_err(|error| io_error("create", path, error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("write", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync", path, error))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

pub(crate) fn read_all(path: &Path) -> Result<Vec<u8>, CoreError> {
    std::fs::read(path).map_err(|error| io_error("read", path, error))
}

pub(crate) fn copy_file(source: &Path, destination: &Path) -> Result<u64, CoreError> {
    std::fs::copy(source, destination).map_err(|error| io_error("copy", destination, error))
}

pub(crate) fn remove_file_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove", path, error)),
    }
}
