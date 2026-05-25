use thiserror::Error;
use crate::video::types::StructuredError;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("License not activated")]
    NotActivated,
    #[error("License key is invalid")]
    InvalidLicenseKey,
    #[error("License key has expired")]
    LicenseExpired,
    #[error("Token is corrupted or tampered")]
    TokenCorrupted,
    #[error("Machine identifier mismatch — license bound to another machine")]
    MachineMismatch,
    #[error("Activation failed: {reason}")]
    ActivationFailed { reason: String },
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
        let code = match &error {
            AuthError::NotActivated => "not_activated",
            AuthError::InvalidLicenseKey => "invalid_license_key",
            AuthError::LicenseExpired => "license_expired",
            AuthError::TokenCorrupted => "token_corrupted",
            AuthError::MachineMismatch => "machine_mismatch",
            AuthError::ActivationFailed { .. } => "activation_failed",
            AuthError::RefreshFailed { .. } => "refresh_failed",
            AuthError::PhaseDNotImplemented => "phase_d_not_implemented",
            AuthError::StorageError(_) => "storage_error",
            AuthError::MachineIdError(_) => "machine_id_error",
            AuthError::JwtError(_) => "jwt_error",
            AuthError::IoError(_) => "io_error",
            AuthError::JsonError(_) => "json_error",
            AuthError::TauriError(_) => "tauri_error",
        }
        .to_string();
        Self {
            code,
            message: error.to_string(),
        }
    }
}


