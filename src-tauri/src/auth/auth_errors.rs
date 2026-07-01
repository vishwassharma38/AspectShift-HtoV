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

pub fn is_transient_refresh_failure(error: &AuthError) -> bool {
    match error {
        AuthError::ServerError => true,
        AuthError::RefreshFailed { reason } => {
            let normalized = reason.trim().to_ascii_lowercase();
            normalized.contains("network error")
                || normalized.contains("timeout")
                || normalized.contains("dns")
                || normalized.contains("failed to read refresh response")
                || normalized.contains("connection")
                || normalized.contains("500")
                || normalized.contains("502")
                || normalized.contains("503")
                || normalized.contains("504")
        }
        AuthError::IoError(_) => true,
        AuthError::TauriError(_) => true,
        // Authoritative failures (must NOT enter grace)
        AuthError::InvalidLicenseKey
        | AuthError::InvalidLicense
        | AuthError::LicenseExpired
        | AuthError::TokenCorrupted
        | AuthError::MachineMismatch
        | AuthError::LicenseNotFound
        | AuthError::LicenseRevoked
        | AuthError::LicenseRefunded
        | AuthError::ActivationLimitReached
        | AuthError::InvalidRequest
        | AuthError::InvalidLicenseTier
        | AuthError::JwtError(_) => false,
        // Storage/Internal errors - generally safer not to enter grace
        AuthError::StorageError(_) | AuthError::MachineIdError(_) | AuthError::JsonError(_) => {
            false
        }
        AuthError::NotActivated
        | AuthError::ActivationFailed { .. }
        | AuthError::PhaseDNotImplemented => false,
    }
}
