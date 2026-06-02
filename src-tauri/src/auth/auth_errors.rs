use thiserror::Error;

use crate::auth::outcome_mapping::structured_error_code_from_error;
use crate::video::types::StructuredError;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("License not activated")]
    NotActivated,
    #[error("License key is invalid")]
    InvalidLicenseKey,
    #[error("License is invalid")]
    InvalidLicense,
    #[error("License key has expired")]
    LicenseExpired,
    #[error("Token is corrupted or tampered")]
    TokenCorrupted,
    #[error("Machine identifier mismatch - license bound to another machine")]
    MachineMismatch,
    #[error("License not found")]
    LicenseNotFound,
    #[error("License has been revoked")]
    LicenseRevoked,
    #[error("License has been refunded")]
    LicenseRefunded,
    #[error("Activation limit reached")]
    ActivationLimitReached,
    #[error("Invalid activation request")]
    InvalidRequest,
    #[error("License server error")]
    ServerError,
    #[error("Activation failed: {reason}")]
    ActivationFailed { reason: String },
    #[error("Invalid or unsupported license tier")]
    InvalidLicenseTier,
    #[error("Refresh failed: {reason}")]
    RefreshFailed { reason: String },
    #[error("Phase D provider path not implemented")]
    PhaseDNotImplemented,
    #[error("Secure storage error: {0}")]
    StorageError(String),
    #[error("Machine ID error: {0}")]
    MachineIdError(String),
    #[error("JWT error: {0}")]
    JwtError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Tauri error: {0}")]
    TauriError(#[from] tauri::Error),
}

impl From<AuthError> for StructuredError {
    fn from(error: AuthError) -> Self {
        Self {
            code: structured_error_code_from_error(&error).to_string(),
            message: error.to_string(),
        }
    }
}
