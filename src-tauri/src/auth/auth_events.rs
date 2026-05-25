use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

use crate::auth::state::auth_state::AuthState;

pub const AUTH_STATUS_CHANGED: &str = "auth://status-changed";
pub const AUTH_ACTIVATION_SUCCESS: &str = "auth://activation-success";
pub const AUTH_ACTIVATION_FAILED: &str = "auth://activation-failed";
pub const AUTH_REFRESH_REQUIRED: &str = "auth://refresh-required";
pub const AUTH_LICENSE_INVALID: &str = "auth://license-invalid";

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatusChangedPayload {
    pub auth_state: AuthState,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthActivationFailedPayload {
    pub reason: String,
    pub error_code: String,
}

pub fn emit_auth_state(app: &AppHandle, auth_state: &AuthState) {
    let _ = app.emit(
        AUTH_STATUS_CHANGED,
        AuthStatusChangedPayload {
            auth_state: auth_state.clone(),
        },
    );
}

pub fn emit_activation_success(app: &AppHandle, auth_state: &AuthState) {
    let _ = app.emit(
        AUTH_ACTIVATION_SUCCESS,
        AuthStatusChangedPayload {
            auth_state: auth_state.clone(),
        },
    );
}

pub fn emit_activation_failed(app: &AppHandle, reason: &str, error_code: &str) {
    let _ = app.emit(
        AUTH_ACTIVATION_FAILED,
        AuthActivationFailedPayload {
            reason: reason.to_string(),
            error_code: error_code.to_string(),
        },
    );
}

pub fn emit_refresh_required(app: &AppHandle, auth_state: &AuthState) {
    let _ = app.emit(
        AUTH_REFRESH_REQUIRED,
        AuthStatusChangedPayload {
            auth_state: auth_state.clone(),
        },
    );
}

pub fn emit_license_invalid(app: &AppHandle, auth_state: &AuthState) {
    let _ = app.emit(
        AUTH_LICENSE_INVALID,
        AuthStatusChangedPayload {
            auth_state: auth_state.clone(),
        },
    );
}


