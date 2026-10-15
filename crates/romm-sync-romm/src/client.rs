use std::{collections::BTreeSet, fmt, sync::Arc, time::Duration};

use md5::{Digest, Md5};
use reqwest::{
    StatusCode,
    blocking::{Client, RequestBuilder, Response, multipart},
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::{Deserialize, de::DeserializeOwned};
use url::Url;

use crate::{
    CompleteSessionRequest, DeviceAuthorization, DeviceAuthorizationRequest, DeviceCredential,
    DeviceTokenRequest, DeviceTokenResponse, DownloadConfirmation, Heartbeat, LocalSaveObservation,
    NegotiateRequest, NegotiateResponse, NoopTrace, REQUIRED_SCOPES, RomMError, SaveRecord,
    TraceEvent, TraceSink, UploadOutcome, VerifiedDownload,
};

const HEARTBEAT_ROUTE: &str = "/api/heartbeat";
const AUTH_INIT_ROUTE: &str = "/api/auth/device/init";
const AUTH_TOKEN_ROUTE: &str = "/api/auth/device/token";
const NEGOTIATE_ROUTE: &str = "/api/sync/negotiate";
const SAVES_ROUTE: &str = "/api/saves";
const SAVE_CONTENT_ROUTE: &str = "/api/saves/{id}/content";
const SAVE_DOWNLOADED_ROUTE: &str = "/api/saves/{id}/downloaded";
const COMPLETE_ROUTE: &str = "/api/sync/sessions/{id}/complete";

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub server_origin: Url,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub allow_insecure_http: bool,
}

impl ClientConfig {
    /// Creates a config with `HTTPS`-only transport and bounded timeouts.
    ///
    /// # Errors
    ///
    /// Returns an error unless `origin` is a root `HTTP(S)` origin without
    /// credentials, query, or fragment.
    pub fn new(origin: &str) -> Result<Self, RomMError> {
        let server_origin = normalize_origin(origin)?;
        Ok(Self {
            server_origin,
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(20),
            allow_insecure_http: false,
        })
    }

    #[must_use]
    pub const fn with_insecure_http(mut self, allow: bool) -> Self {
        self.allow_insecure_http = allow;
        self
    }
}

pub struct RomMClient {
    config: ClientConfig,
    http: Client,
    credential: Option<DeviceCredential>,
    trace: Arc<dyn TraceSink>,
}

impl fmt::Debug for RomMClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RomMClient")
            .field("server_origin", &self.config.server_origin.as_str())
            .field("authenticated", &self.credential.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub enum TokenPoll {
    Pending,
    Authorized(DeviceCredential),
}

impl RomMClient {
    /// Builds an unauthenticated client used only for heartbeat and device
    /// authorization.
    ///
    /// # Errors
    ///
    /// Returns an error for insecure transport or `HTTP` client construction.
    pub fn for_pairing(config: ClientConfig) -> Result<Self, RomMError> {
        Self::build(config, None, Arc::new(NoopTrace))
    }

    /// Builds an authenticated, device-bound client after exact-scope and
    /// server-origin validation.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid credentials, origin mismatch, insecure
    /// transport, or `HTTP` client construction.
    pub fn authenticated(
        config: ClientConfig,
        credential: DeviceCredential,
    ) -> Result<Self, RomMError> {
        credential.validate()?;
        if credential.server_origin != config.server_origin.as_str().trim_end_matches('/') {
            return Err(RomMError::CredentialOriginMismatch);
        }
        Self::build(config, Some(credential), Arc::new(NoopTrace))
    }

    /// Replaces the trace sink. Trace events contain only method, route
    /// template, and status.
    #[must_use]
    pub fn with_trace_sink(mut self, trace: Arc<dyn TraceSink>) -> Self {
        self.trace = trace;
        self
    }

    fn build(
        config: ClientConfig,
        credential: Option<DeviceCredential>,
        trace: Arc<dyn TraceSink>,
    ) -> Result<Self, RomMError> {
        if config.server_origin.scheme() == "http" && !config.allow_insecure_http {
            return Err(RomMError::InsecureTransportDisabled);
        }
        let mut headers = HeaderMap::new();
        if let Some(credential) = &credential {
            let mut value = HeaderValue::from_str(&format!("Bearer {}", credential.token()))
                .map_err(|_| {
                    RomMError::CredentialStorage("token is not a valid header".to_owned())
                })?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        let http = Client::builder()
            .default_headers(headers)
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .build()
            .map_err(|source| RomMError::Transport {
                route: "client construction",
                source,
            })?;
        Ok(Self {
            config,
            http,
            credential,
            trace,
        })
    }

    /// Reads the live version and fails closed outside `RomM` 5.4.x.
    ///
    /// # Errors
    ///
    /// Returns an error for transport/status/schema failures or version drift.
    pub fn ensure_compatible(&self) -> Result<String, RomMError> {
        let url = self.endpoint(HEARTBEAT_ROUTE)?;
        let response = self.send("GET", HEARTBEAT_ROUTE, self.http.get(url))?;
        let heartbeat: Heartbeat = decode_json(response, HEARTBEAT_ROUTE)?;
        let mut parts = heartbeat.system.version.split('.');
        if parts.next() != Some("5") || parts.next() != Some("4") {
            return Err(RomMError::UnsupportedVersion {
                found: heartbeat.system.version,
            });
        }
        Ok(heartbeat.system.version)
    }

    /// Starts `RomM`'s device authorization flow with the exact M2 scopes.
    ///
    /// # Errors
    ///
    /// Returns an error for compatibility, transport, status, or schema
    /// failures.
    pub fn begin_device_authorization(
        &self,
        mut request: DeviceAuthorizationRequest,
    ) -> Result<DeviceAuthorization, RomMError> {
        self.ensure_compatible()?;
        request.requested_scopes = REQUIRED_SCOPES
            .iter()
            .map(|scope| (*scope).to_owned())
            .collect();
        let url = self.endpoint(AUTH_INIT_ROUTE)?;
        let response = self.send("POST", AUTH_INIT_ROUTE, self.http.post(url).json(&request))?;
        decode_json(response, AUTH_INIT_ROUTE)
    }

    /// Polls once for device authorization. Pending responses are not treated
    /// as failures; the caller must honor `RomM`'s supplied interval and expiry.
    ///
    /// # Errors
    ///
    /// Returns an error for terminal `HTTP`/schema failures or a scope mismatch.
    pub fn poll_device_token(&self, device_code: &str) -> Result<TokenPoll, RomMError> {
        let url = self.endpoint(AUTH_TOKEN_ROUTE)?;
        let response = self
            .http
            .post(url)
            .json(&DeviceTokenRequest {
                device_code: device_code.to_owned(),
            })
            .send()
            .map_err(|source| RomMError::Transport {
                route: AUTH_TOKEN_ROUTE,
                source,
            })?;
        self.trace.record(TraceEvent {
            method: "POST",
            route: AUTH_TOKEN_ROUTE,
            status: Some(response.status().as_u16()),
        });
        if matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::TOO_EARLY
        ) {
            return Ok(TokenPoll::Pending);
        }
        let response = require_success(response, AUTH_TOKEN_ROUTE)?;
        let token: DeviceTokenResponse = decode_json(response, AUTH_TOKEN_ROUTE)?;
        let credential = DeviceCredential::new(
            self.config.server_origin.as_str().trim_end_matches('/'),
            token.device_id,
            token.scopes,
            token.expires_at,
            token.access_token,
        )?;
        Ok(TokenPoll::Authorized(credential))
    }

    /// Negotiates one bounded sync session. Server-proposed directions remain
    /// advisory; the pure planner must still apply last-common-version rules.
    ///
    /// # Errors
    ///
    /// Returns an error for compatibility, invalid observations, or API
    /// failures.
    pub fn negotiate(
        &self,
        saves: Vec<LocalSaveObservation>,
        rom_ids: Option<Vec<i64>>,
        emulators: Option<Vec<String>>,
    ) -> Result<NegotiateResponse, RomMError> {
        self.ensure_compatible()?;
        for save in &saves {
            validate_observation(save)?;
        }
        let credential = self.credential()?;
        let request = NegotiateRequest {
            device_id: credential.device_id.clone(),
            saves,
            rom_ids,
            restore_unlisted: false,
            emulators,
        };
        let url = self.endpoint(NEGOTIATE_ROUTE)?;
        let response = self.send("POST", NEGOTIATE_ROUTE, self.http.post(url).json(&request))?;
        let negotiated: NegotiateResponse = decode_json(response, NEGOTIATE_ROUTE)?;
        validate_negotiation(&negotiated)?;
        Ok(negotiated)
    }

    /// Uploads an autosave with `RomM`'s conditional-write guard. A 409 is never
    /// overwritten. If a response is lost, an identical visible slot head is
    /// accepted as an idempotent success rather than uploaded again.
    ///
    /// # Errors
    ///
    /// Returns an error for version drift, conflict, transport, or integrity
    /// failure.
    pub fn upload_autosave(
        &self,
        rom_id: i64,
        session_id: i64,
        slot: &str,
        file_name: &str,
        emulator: Option<&str>,
        bytes: Vec<u8>,
    ) -> Result<UploadOutcome, RomMError> {
        self.ensure_compatible()?;
        let credential = self.credential()?;
        validate_slot(slot)?;
        let digest = md5_hex(&bytes);
        if let Some(existing) = self.latest_save(rom_id, slot)?
            && existing.content_hash.as_deref() == Some(digest.as_str())
        {
            return Ok(UploadOutcome::AlreadyPresent(existing));
        }
        let url = self.endpoint(SAVES_ROUTE)?;
        let form = multipart::Form::new().part(
            "saveFile",
            multipart::Part::bytes(bytes)
                .file_name(file_name.to_owned())
                .mime_str("application/octet-stream")
                .map_err(|source| RomMError::Transport {
                    route: SAVES_ROUTE,
                    source,
                })?,
        );
        let mut query = vec![
            ("rom_id", rom_id.to_string()),
            ("slot", slot.to_owned()),
            ("device_id", credential.device_id.clone()),
            ("session_id", session_id.to_string()),
            ("content_hash", digest.clone()),
            ("overwrite", "false".to_owned()),
            ("autocleanup", "true".to_owned()),
            ("autocleanup_limit", "10".to_owned()),
        ];
        if let Some(emulator) = emulator {
            query.push(("emulator", emulator.to_owned()));
        }
        let request = self.http.post(url).query(&query).multipart(form);
        let response = match request.send() {
            Ok(response) => response,
            Err(source) => {
                if let Ok(Some(existing)) = self.latest_save(rom_id, slot)
                    && existing.content_hash.as_deref() == Some(digest.as_str())
                {
                    return Ok(UploadOutcome::AlreadyPresent(existing));
                }
                return Err(RomMError::Transport {
                    route: SAVES_ROUTE,
                    source,
                });
            }
        };
        self.trace.record(TraceEvent {
            method: "POST",
            route: SAVES_ROUTE,
            status: Some(response.status().as_u16()),
        });
        if response.status() == StatusCode::CONFLICT {
            if let Some(existing) = self.latest_save(rom_id, slot)?
                && existing.content_hash.as_deref() == Some(digest.as_str())
            {
                return Ok(UploadOutcome::AlreadyPresent(existing));
            }
            return Err(RomMError::Conflict);
        }
        let stored: SaveRecord = decode_json(require_success(response, SAVES_ROUTE)?, SAVES_ROUTE)?;
        validate_stored_save(&stored, rom_id, slot, &digest)?;
        Ok(UploadOutcome::Stored(stored))
    }

    /// Downloads without optimistic acknowledgement and verifies `RomM`'s `MD5`
    /// and declared size before returning bytes to M1B staging.
    ///
    /// # Errors
    ///
    /// Returns an error for version drift, API failure, or byte-integrity
    /// mismatch.
    pub fn download_unconfirmed(
        &self,
        save: SaveRecord,
        session_id: i64,
    ) -> Result<VerifiedDownload, RomMError> {
        self.ensure_compatible()?;
        let credential = self.credential()?;
        let url = self.endpoint(&format!("/api/saves/{}/content", save.id))?;
        let query = [
            ("device_id", credential.device_id.clone()),
            ("session_id", session_id.to_string()),
            ("optimistic", "false".to_owned()),
        ];
        let response = self.send("GET", SAVE_CONTENT_ROUTE, self.http.get(url).query(&query))?;
        let bytes = response
            .bytes()
            .map_err(|source| RomMError::Transport {
                route: SAVE_CONTENT_ROUTE,
                source,
            })?
            .to_vec();
        let digest = md5_hex(&bytes);
        if save.content_hash.as_deref() != Some(digest.as_str()) {
            return Err(RomMError::Integrity(
                "download MD5 differs from negotiated server hash".to_owned(),
            ));
        }
        if let Some(expected_size) = save.file_size
            && expected_size != bytes.len() as u64
        {
            return Err(RomMError::Integrity(
                "download size differs from server metadata".to_owned(),
            ));
        }
        Ok(VerifiedDownload {
            save,
            bytes,
            md5: digest,
        })
    }

    /// Confirms a download only after the caller has durably committed and
    /// re-read the local bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for version drift or API failure.
    pub fn confirm_download(&self, save_id: i64, md5: &str) -> Result<(), RomMError> {
        self.ensure_compatible()?;
        validate_md5(md5)?;
        let credential = self.credential()?;
        let url = self.endpoint(&format!("/api/saves/{save_id}/downloaded"))?;
        self.send(
            "POST",
            SAVE_DOWNLOADED_ROUTE,
            self.http.post(url).json(&DownloadConfirmation {
                device_id: &credential.device_id,
                content_hash: md5,
            }),
        )?;
        Ok(())
    }

    /// Completes a negotiated session with explicit success/failure counts.
    /// Repeating the same completion request is safe under the `RomM` 5.4
    /// session contract.
    ///
    /// # Errors
    ///
    /// Returns an error for version drift or API failure.
    pub fn complete_session(
        &self,
        session_id: i64,
        operations_completed: u64,
        operations_failed: u64,
    ) -> Result<(), RomMError> {
        self.ensure_compatible()?;
        let _credential = self.credential()?;
        let url = self.endpoint(&format!("/api/sync/sessions/{session_id}/complete"))?;
        self.send(
            "POST",
            COMPLETE_ROUTE,
            self.http.post(url).json(&CompleteSessionRequest {
                operations_completed,
                operations_failed,
            }),
        )?;
        Ok(())
    }

    /// Returns the current head for a `RomM` slot from the direct save listing.
    /// Superseded versions remain visible but do not become the head.
    ///
    /// # Errors
    ///
    /// Returns an error for API or schema failure.
    pub fn latest_save(&self, rom_id: i64, slot: &str) -> Result<Option<SaveRecord>, RomMError> {
        validate_slot(slot)?;
        Ok(self
            .list_saves(rom_id)?
            .into_iter()
            .filter(|record| record.rom_id == rom_id && record.slot.as_deref() == Some(slot))
            .max_by(|left, right| {
                left.updated_at
                    .cmp(&right.updated_at)
                    .then(left.id.cmp(&right.id))
            }))
    }

    /// Lists the caller's retained save versions for one ROM.
    ///
    /// # Errors
    ///
    /// Returns an error for API or schema failure.
    pub fn list_saves(&self, rom_id: i64) -> Result<Vec<SaveRecord>, RomMError> {
        let _credential = self.credential()?;
        let url = self.endpoint(SAVES_ROUTE)?;
        let response = self.send(
            "GET",
            SAVES_ROUTE,
            self.http.get(url).query(&[("rom_id", rom_id)]),
        )?;
        let records: SaveList = decode_json(response, SAVES_ROUTE)?;
        Ok(records
            .into_records()
            .into_iter()
            .filter(|record| record.rom_id == rom_id)
            .collect())
    }

    fn credential(&self) -> Result<&DeviceCredential, RomMError> {
        self.credential.as_ref().ok_or(RomMError::MissingCredential)
    }

    fn endpoint(&self, path: &str) -> Result<Url, RomMError> {
        self.config
            .server_origin
            .join(path.trim_start_matches('/'))
            .map_err(|error| RomMError::InvalidOrigin(error.to_string()))
    }

    fn send(
        &self,
        method: &'static str,
        route: &'static str,
        request: RequestBuilder,
    ) -> Result<Response, RomMError> {
        let result = request.send();
        match result {
            Ok(response) => {
                self.trace.record(TraceEvent {
                    method,
                    route,
                    status: Some(response.status().as_u16()),
                });
                require_success(response, route)
            }
            Err(source) => {
                self.trace.record(TraceEvent {
                    method,
                    route,
                    status: None,
                });
                Err(RomMError::Transport { route, source })
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SaveList {
    Direct(Vec<SaveRecord>),
    Items { items: Vec<SaveRecord> },
}

impl SaveList {
    fn into_records(self) -> Vec<SaveRecord> {
        match self {
            Self::Direct(records) | Self::Items { items: records } => records,
        }
    }
}

fn normalize_origin(origin: &str) -> Result<Url, RomMError> {
    let mut url =
        Url::parse(origin).map_err(|error| RomMError::InvalidOrigin(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(RomMError::InvalidOrigin(
            "expected a root HTTP(S) origin without credentials/query/fragment".to_owned(),
        ));
    }
    url.set_path("/");
    Ok(url)
}

fn require_success(response: Response, route: &'static str) -> Result<Response, RomMError> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(RomMError::Api {
            route,
            status: response.status().as_u16(),
        })
    }
}

fn decode_json<T: DeserializeOwned>(
    response: Response,
    route: &'static str,
) -> Result<T, RomMError> {
    response
        .json()
        .map_err(|source| RomMError::Transport { route, source })
}

fn validate_observation(save: &LocalSaveObservation) -> Result<(), RomMError> {
    validate_slot(&save.slot)?;
    validate_md5(&save.content_hash)?;
    if save.file_name.trim().is_empty() || save.updated_at.trim().is_empty() {
        return Err(RomMError::Protocol(
            "local save filename and updated_at are mandatory".to_owned(),
        ));
    }
    Ok(())
}

fn validate_negotiation(response: &NegotiateResponse) -> Result<(), RomMError> {
    let declared = response.total_upload
        + response.total_download
        + response.total_conflict
        + response.total_no_op
        + response.total_delete;
    if declared != response.operations.len() as u64 {
        return Err(RomMError::Protocol(
            "operation totals do not equal the operation list".to_owned(),
        ));
    }
    for operation in &response.operations {
        let slot = operation.slot.as_deref().ok_or_else(|| {
            RomMError::Protocol("null save slots are not automatic-sync candidates".to_owned())
        })?;
        validate_slot(slot)?;
        if matches!(
            operation.action,
            crate::OperationAction::Download | crate::OperationAction::Conflict
        ) && (operation.save_id.is_none() || operation.server_content_hash.is_none())
        {
            return Err(RomMError::Protocol(
                "download/conflict operation lacks save identity or hash".to_owned(),
            ));
        }
        if let Some(hash) = &operation.server_content_hash {
            validate_md5(hash)?;
        }
    }
    Ok(())
}

fn validate_stored_save(
    save: &SaveRecord,
    rom_id: i64,
    slot: &str,
    digest: &str,
) -> Result<(), RomMError> {
    if save.rom_id != rom_id
        || save.slot.as_deref() != Some(slot)
        || save.content_hash.as_deref() != Some(digest)
    {
        return Err(RomMError::Integrity(
            "upload receipt differs from the submitted ROM, slot, or MD5".to_owned(),
        ));
    }
    Ok(())
}

fn validate_slot(slot: &str) -> Result<(), RomMError> {
    if slot != "autosave" {
        return Err(RomMError::Protocol(format!(
            "automatic sync requires the explicit autosave slot, got {slot:?}"
        )));
    }
    Ok(())
}

fn validate_md5(value: &str) -> Result<(), RomMError> {
    if value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(RomMError::Protocol(
            "expected a 32-character MD5 hex digest".to_owned(),
        ))
    }
}

#[must_use]
pub fn md5_hex(bytes: &[u8]) -> String {
    hex_lower(&Md5::digest(bytes))
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[must_use]
pub fn exact_scope_set() -> BTreeSet<String> {
    REQUIRED_SCOPES
        .iter()
        .map(|scope| (*scope).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ClientConfig, md5_hex};

    #[test]
    fn origins_and_wire_hashes_are_canonical() {
        let config = ClientConfig::new("https://romm.example:8443").expect("origin");
        assert_eq!(config.server_origin.as_str(), "https://romm.example:8443/");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert!(ClientConfig::new("https://user@romm.example").is_err());
        assert!(ClientConfig::new("https://romm.example/subpath").is_err());
    }
}
