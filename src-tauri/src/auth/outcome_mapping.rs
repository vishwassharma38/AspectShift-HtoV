use crate::auth::auth_errors::AuthError;
use crate::auth::state::auth_state::AuthStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthOutcomeMapping {
    pub status: AuthStatus,
    pub error_code: &'static str,
}

impl AuthOutcomeMapping {
    pub const fn new(status: AuthStatus, error_code: &'static str) -> Self {
        Self { status, error_code }
    }
}

fn reason_contains(reason: &str, needle: &str) -> bool {
    reason
        .trim()
        .to_ascii_uppercase()
        .contains(&needle.to_ascii_uppercase())
}

fn map_backend_reason(reason: &str) -> AuthOutcomeMapping {
    if reason_contains(reason, "MACHINE_MISMATCH") {
        AuthOutcomeMapping::new(AuthStatus::MachineMismatch, "machine_mismatch")
    } else if reason_contains(reason, "INVALID_TOKEN") || reason_contains(reason, "TOKEN_CORRUPT") {
        AuthOutcomeMapping::new(AuthStatus::Corrupted, "token_corrupted")
    } else if reason_contains(reason, "LICENSE_NOT_FOUND") {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "invalid_license")
    } else if reason_contains(reason, "LICENSE_REVOKED") {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "license_revoked")
    } else if reason_contains(reason, "LICENSE_REFUNDED") {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "license_refunded")
    } else if reason_contains(reason, "LICENSE_MAX_MACHINES") {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "license_max_machines")
    } else if reason_contains(reason, "ACTIVATION_LIMIT_REACHED") {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "activation_limit_reached")
    } else if reason_contains(reason, "INVALID_REQUEST") {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "invalid_request")
    } else if reason_contains(reason, "SERVER_ERROR") {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "server_error")
    } else if reason_contains(reason, "LICENSE_EXPIRED") {
        AuthOutcomeMapping::new(AuthStatus::Expired, "license_expired")
    } else {
        AuthOutcomeMapping::new(AuthStatus::Invalid, "activation_failed")
    }
}

pub fn map_auth_error(error: &AuthError) -> AuthOutcomeMapping {
    match error {
        AuthError::NotActivated => {
            AuthOutcomeMapping::new(AuthStatus::NotActivated, "not_activated")
        }
        AuthError::InvalidLicenseKey => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "invalid_license_key")
        }
        AuthError::InvalidLicense => AuthOutcomeMapping::new(AuthStatus::Invalid, "invalid_license"),
        AuthError::InvalidLicenseTier => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "invalid_license_tier")
        }
        AuthError::LicenseExpired => {
            AuthOutcomeMapping::new(AuthStatus::Expired, "license_expired")
        }
        AuthError::TokenCorrupted => {
            AuthOutcomeMapping::new(AuthStatus::Corrupted, "token_corrupted")
        }
        AuthError::MachineMismatch => {
            AuthOutcomeMapping::new(AuthStatus::MachineMismatch, "machine_mismatch")
        }
        AuthError::LicenseNotFound => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "invalid_license")
        }
        AuthError::LicenseRevoked => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "license_revoked")
        }
        AuthError::LicenseRefunded => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "license_refunded")
        }
        AuthError::ActivationLimitReached => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "activation_limit_reached")
        }
        AuthError::InvalidRequest => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "invalid_request")
        }
        AuthError::ServerError => AuthOutcomeMapping::new(AuthStatus::Invalid, "server_error"),
        AuthError::ActivationFailed { reason } => map_backend_reason(reason),
        AuthError::RefreshFailed { reason } => {
            let mapped = map_backend_reason(reason);
            if mapped.error_code == "activation_failed" {
                AuthOutcomeMapping::new(mapped.status, "refresh_failed")
            } else {
                mapped
            }
        }
        AuthError::PhaseDNotImplemented => {
            AuthOutcomeMapping::new(AuthStatus::RefreshRequired, "phase_d_not_implemented")
        }
        AuthError::StorageError(_) => AuthOutcomeMapping::new(AuthStatus::Invalid, "storage_error"),
        AuthError::MachineIdError(_) => {
            AuthOutcomeMapping::new(AuthStatus::Invalid, "machine_id_error")
        }
        AuthError::JwtError(_) => AuthOutcomeMapping::new(AuthStatus::Corrupted, "jwt_error"),
        AuthError::IoError(_) => AuthOutcomeMapping::new(AuthStatus::Invalid, "io_error"),
        AuthError::JsonError(_) => AuthOutcomeMapping::new(AuthStatus::Corrupted, "json_error"),
        AuthError::TauriError(_) => AuthOutcomeMapping::new(AuthStatus::Invalid, "tauri_error"),
    }
}

pub fn auth_status_from_error(error: &AuthError) -> AuthStatus {
    map_auth_error(error).status
}

pub fn structured_error_code_from_error(error: &AuthError) -> &'static str {
    map_auth_error(error).error_code
}
