use thiserror::Error;

#[derive(Debug, Error)]
pub enum RomMError {
    #[error("invalid RomM server origin: {0}")]
    InvalidOrigin(String),
    #[error("plain HTTP is disabled; explicitly opt in only for a trusted LAN deployment")]
    InsecureTransportDisabled,
    #[error("RomM {found} is outside the supported 5.4 compatibility band")]
    UnsupportedVersion { found: String },
    #[error("credential scopes differ from the exact least-privilege M2 scope set")]
    ScopeMismatch,
    #[error("credential server origin does not match the configured server")]
    CredentialOriginMismatch,
    #[error("authenticated operation has no device credential")]
    MissingCredential,
    #[error("credential storage rejected: {0}")]
    CredentialStorage(String),
    #[error("HTTP transport failed for {route}: {source}")]
    Transport {
        route: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("RomM returned HTTP {status} for {route}")]
    Api { route: &'static str, status: u16 },
    #[error("RomM rejected a conditional upload because the slot changed")]
    Conflict,
    #[error("RomM response did not satisfy the 5.4 contract: {0}")]
    Protocol(String),
    #[error("transferred save failed integrity validation: {0}")]
    Integrity(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl RomMError {
    #[must_use]
    pub const fn retryable(&self) -> bool {
        match self {
            Self::Transport { .. } => true,
            Self::Api { status, .. } => *status == 408 || *status == 429 || *status >= 500,
            Self::InvalidOrigin(_)
            | Self::InsecureTransportDisabled
            | Self::UnsupportedVersion { .. }
            | Self::ScopeMismatch
            | Self::CredentialOriginMismatch
            | Self::MissingCredential
            | Self::CredentialStorage(_)
            | Self::Conflict
            | Self::Protocol(_)
            | Self::Integrity(_)
            | Self::Json(_)
            | Self::Io(_) => false,
        }
    }
}
