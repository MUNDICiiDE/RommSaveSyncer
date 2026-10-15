use serde::{Deserialize, Serialize};

pub const REQUIRED_SCOPES: [&str; 6] = [
    "assets.read",
    "assets.write",
    "devices.read",
    "devices.write",
    "platforms.read",
    "roms.read",
];

#[derive(Clone, Debug, Serialize)]
pub struct DeviceAuthorizationRequest {
    pub client_device_identifier: String,
    pub name: String,
    pub client: String,
    pub client_version: String,
    pub platform: String,
    pub requested_scopes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    #[serde(alias = "verification_uri", alias = "verification_url")]
    pub verification_path: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceTokenRequest {
    pub device_code: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceTokenResponse {
    #[serde(alias = "token")]
    pub access_token: String,
    pub device_id: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default, alias = "expires_at")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Heartbeat {
    #[serde(rename = "SYSTEM")]
    pub system: HeartbeatSystem,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HeartbeatSystem {
    #[serde(rename = "VERSION")]
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct LocalSaveObservation {
    pub rom_id: i64,
    pub file_name: String,
    pub slot: String,
    pub emulator: Option<String>,
    pub content_hash: String,
    pub updated_at: String,
    pub file_size_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct NegotiateRequest {
    pub device_id: String,
    pub saves: Vec<LocalSaveObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rom_ids: Option<Vec<i64>>,
    pub restore_unlisted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emulators: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OperationAction {
    Upload,
    Download,
    Conflict,
    Delete,
    NoOp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SyncOperation {
    pub action: OperationAction,
    pub rom_id: i64,
    pub save_id: Option<i64>,
    pub file_name: String,
    pub slot: Option<String>,
    pub emulator: Option<String>,
    pub reason: String,
    pub server_updated_at: Option<String>,
    pub server_content_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct NegotiateResponse {
    pub session_id: i64,
    pub operations: Vec<SyncOperation>,
    pub total_upload: u64,
    pub total_download: u64,
    pub total_conflict: u64,
    pub total_no_op: u64,
    pub total_delete: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SaveRecord {
    pub id: i64,
    pub rom_id: i64,
    #[serde(default)]
    pub file_name: String,
    pub slot: Option<String>,
    #[serde(default)]
    pub emulator: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default, alias = "file_size_bytes")]
    pub file_size: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default, alias = "origin_device_id")]
    pub device_id: Option<String>,
    #[serde(default, alias = "device")]
    pub origin_device: Option<OriginDevice>,
}

impl SaveRecord {
    #[must_use]
    pub fn revision(&self) -> String {
        format!(
            "{}:{}:{}",
            self.id,
            self.updated_at.as_deref().unwrap_or("unknown"),
            self.content_hash.as_deref().unwrap_or("unknown")
        )
    }

    #[must_use]
    pub fn origin_device_id(&self) -> Option<&str> {
        self.device_id.as_deref().or_else(|| {
            self.origin_device.as_ref().and_then(|origin| match origin {
                OriginDevice::Id(id) => Some(id.as_str()),
                OriginDevice::Object { id, device_id, .. } => {
                    device_id.as_deref().or(id.as_deref())
                }
            })
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum OriginDevice {
    Id(String),
    Object {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        device_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct DownloadConfirmation<'a> {
    pub device_id: &'a str,
    pub content_hash: &'a str,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct CompleteSessionRequest {
    pub operations_completed: u64,
    pub operations_failed: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDownload {
    pub save: SaveRecord,
    pub bytes: Vec<u8>,
    pub md5: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UploadOutcome {
    Stored(SaveRecord),
    AlreadyPresent(SaveRecord),
}
