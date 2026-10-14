use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{
    fs,
    io::{Read, Write},
    path::Component,
};

use serde::{Deserialize, Serialize};

use crate::{REQUIRED_SCOPES, RomMError};

pub const CREDENTIAL_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceCredential {
    pub schema_version: u32,
    pub server_origin: String,
    pub device_id: String,
    pub granted_scopes: BTreeSet<String>,
    pub expires_at: Option<String>,
    access_token: String,
}

impl fmt::Debug for DeviceCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceCredential")
            .field("schema_version", &self.schema_version)
            .field("server_origin", &self.server_origin)
            .field("device_id", &self.device_id)
            .field("granted_scopes", &self.granted_scopes)
            .field("expires_at", &self.expires_at)
            .field("access_token", &"[REDACTED]")
            .finish()
    }
}

impl DeviceCredential {
    /// Builds a credential only when `RomM` granted the exact approved scope set.
    ///
    /// # Errors
    ///
    /// Returns an error for extra/missing scopes, an empty token/device ID, or
    /// an unsupported schema.
    pub fn new(
        server_origin: impl Into<String>,
        device_id: impl Into<String>,
        granted_scopes: impl IntoIterator<Item = String>,
        expires_at: Option<String>,
        access_token: impl Into<String>,
    ) -> Result<Self, RomMError> {
        let credential = Self {
            schema_version: CREDENTIAL_SCHEMA_VERSION,
            server_origin: server_origin.into(),
            device_id: device_id.into(),
            granted_scopes: granted_scopes.into_iter().collect(),
            expires_at,
            access_token: access_token.into(),
        };
        credential.validate()?;
        Ok(credential)
    }

    pub(crate) fn token(&self) -> &str {
        &self.access_token
    }

    /// Validates fields without revealing the token.
    ///
    /// # Errors
    ///
    /// Returns an error when any credential invariant is violated.
    pub fn validate(&self) -> Result<(), RomMError> {
        if self.schema_version != CREDENTIAL_SCHEMA_VERSION {
            return Err(RomMError::CredentialStorage(format!(
                "unsupported credential schema {}",
                self.schema_version
            )));
        }
        if self.device_id.trim().is_empty() || self.access_token.trim().is_empty() {
            return Err(RomMError::CredentialStorage(
                "device ID and token must be non-empty".to_owned(),
            ));
        }
        let expected: BTreeSet<_> = REQUIRED_SCOPES
            .iter()
            .map(|scope| (*scope).to_owned())
            .collect();
        if self.granted_scopes != expected {
            return Err(RomMError::ScopeMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stores credentials atomically with a `0700` parent and `0600` file.
    ///
    /// # Errors
    ///
    /// Returns an error for symlinks, wrong ownership/type/mode, serialization,
    /// or durable write failures. Credential files are supported only on Unix.
    pub fn save(&self, credential: &DeviceCredential) -> Result<(), RomMError> {
        credential.validate()?;
        save_platform(&self.path, credential)
    }

    /// Loads credentials after owner, file type, mode, and symlink checks.
    ///
    /// # Errors
    ///
    /// Returns an error for insecure metadata, invalid JSON, or credential
    /// invariant failures. Credential files are supported only on Unix.
    pub fn load(&self) -> Result<DeviceCredential, RomMError> {
        let credential = load_platform(&self.path)?;
        credential.validate()?;
        Ok(credential)
    }
}

#[cfg(unix)]
fn save_platform(path: &Path, credential: &DeviceCredential) -> Result<(), RomMError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let parent = path
        .parent()
        .ok_or_else(|| RomMError::CredentialStorage("credential path has no parent".to_owned()))?;
    reject_symlink_components(parent)?;
    fs::create_dir_all(parent)?;
    reject_symlink_components(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.file_type().is_dir() || metadata.uid() != current_uid() {
        return Err(RomMError::CredentialStorage(
            "credential parent must be an owned directory".to_owned(),
        ));
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;

    match fs::symlink_metadata(path) {
        Ok(_) => validate_file_metadata(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let mut temporary = tempfile::Builder::new()
        .prefix(".credentials-")
        .tempfile_in(parent)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer(temporary.as_file_mut(), credential)?;
    temporary.as_file_mut().write_all(b"\n")?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()?;
    validate_file_metadata(path)
}

#[cfg(not(unix))]
fn save_platform(_path: &Path, _credential: &DeviceCredential) -> Result<(), RomMError> {
    Err(RomMError::CredentialStorage(
        "secure credential files require the Bazzite/Unix target".to_owned(),
    ))
}

#[cfg(unix)]
fn load_platform(path: &Path) -> Result<DeviceCredential, RomMError> {
    use rustix::fs::{Mode, OFlags, open};

    let parent = path
        .parent()
        .ok_or_else(|| RomMError::CredentialStorage("credential path has no parent".to_owned()))?;
    reject_symlink_components(parent)?;
    validate_file_metadata(path)?;
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| RomMError::CredentialStorage(error.to_string()))?;
    let mut file = fs::File::from(descriptor);
    let mut encoded = Vec::new();
    file.read_to_end(&mut encoded)?;
    serde_json::from_slice(&encoded).map_err(RomMError::from)
}

#[cfg(not(unix))]
fn load_platform(_path: &Path) -> Result<DeviceCredential, RomMError> {
    Err(RomMError::CredentialStorage(
        "secure credential files require the Bazzite/Unix target".to_owned(),
    ))
}

#[cfg(unix)]
fn validate_file_metadata(path: &Path) -> Result<(), RomMError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(RomMError::CredentialStorage(
            "credential path must be a regular file, never a symlink".to_owned(),
        ));
    }
    if metadata.uid() != current_uid() {
        return Err(RomMError::CredentialStorage(
            "credential file is not owned by the current user".to_owned(),
        ));
    }
    if metadata.mode() & 0o777 != 0o600 {
        return Err(RomMError::CredentialStorage(
            "credential file mode must be exactly 0600".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn current_uid() -> u32 {
    rustix::process::geteuid().as_raw()
}

#[cfg(unix)]
fn reject_symlink_components(path: &Path) -> Result<(), RomMError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(RomMError::CredentialStorage(
                    "credential path must not contain parent traversal".to_owned(),
                ));
            }
            Component::Normal(segment) => {
                current.push(segment);
                if current.exists() && fs::symlink_metadata(&current)?.file_type().is_symlink() {
                    return Err(RomMError::CredentialStorage(format!(
                        "credential path contains symlink component {}",
                        current.display()
                    )));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::DeviceCredential;
    use crate::{REQUIRED_SCOPES, RomMError};

    fn scopes() -> BTreeSet<String> {
        REQUIRED_SCOPES
            .iter()
            .map(|scope| (*scope).to_owned())
            .collect()
    }

    #[test]
    fn debug_redacts_token_and_exact_scopes_are_required() {
        let credential = DeviceCredential::new(
            "https://romm.example",
            "device",
            scopes(),
            None,
            "not-a-real-super-secret",
        )
        .expect("credential");
        let debug = format!("{credential:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("super-secret"));

        let error = DeviceCredential::new(
            "https://romm.example",
            "device",
            ["assets.read".to_owned()],
            None,
            "token",
        )
        .expect_err("narrow/incomplete credential must fail");
        assert!(matches!(error, RomMError::ScopeMismatch));
    }

    #[cfg(unix)]
    #[test]
    fn store_enforces_modes_and_rejects_symlinks() {
        use std::{
            fs,
            os::unix::fs::{MetadataExt, symlink},
        };

        use super::CredentialStore;

        let temp = tempfile::TempDir::new().expect("temp");
        let path = temp.path().join("config/romm-autosync/credentials.json");
        let store = CredentialStore::new(path.clone());
        let credential = DeviceCredential::new(
            "https://romm.example",
            "device",
            scopes(),
            None,
            "not-a-real-super-secret",
        )
        .expect("credential");
        store.save(&credential).expect("secure save");
        assert_eq!(
            fs::metadata(path.parent().expect("parent"))
                .expect("parent metadata")
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).expect("file metadata").mode() & 0o777,
            0o600
        );
        assert_eq!(store.load().expect("secure load").device_id, "device");

        fs::remove_file(&path).expect("remove credential");
        let target = temp.path().join("elsewhere");
        fs::write(&target, b"not a credential").expect("target");
        symlink(&target, &path).expect("symlink");
        assert!(store.load().is_err());
        assert_eq!(
            fs::metadata(&target).expect("target metadata").uid(),
            rustix::process::geteuid().as_raw()
        );
    }
}
