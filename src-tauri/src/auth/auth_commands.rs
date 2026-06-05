use tauri::{AppHandle, State};

use crate::auth::auth_models::ActivationResult;
use crate::auth::manager::auth_manager::AuthManager;
use crate::auth::state::auth_state::AuthState;
use crate::auth::auth_models::UpdateEntitlementCheckResult;
use crate::video::types::StructuredError;

#[tauri::command]
pub async fn get_auth_state(manager: State<'_, AuthManager>) -> Result<AuthState, StructuredError> {
    Ok(manager.get_auth_state().await)
}

#[tauri::command]
pub async fn activate_license(
    app: AppHandle,
    license_key: String,
    manager: State<'_, AuthManager>,
) -> Result<ActivationResult, StructuredError> {
    manager
        .activate(&app, &license_key)
        .await
        .map_err(StructuredError::from)
}

#[tauri::command]
pub async fn refresh_license(
    app: AppHandle,
    manager: State<'_, AuthManager>,
) -> Result<AuthState, StructuredError> {
    manager.refresh(&app).await.map_err(StructuredError::from)
}

#[tauri::command]
pub async fn check_update_entitlement(
    app: AppHandle,
    manager: State<'_, AuthManager>,
) -> Result<UpdateEntitlementCheckResult, StructuredError> {
    Ok(manager.check_update_entitlement(&app).await)
}

#[tauri::command]
pub async fn clear_license(
    app: AppHandle,
    manager: State<'_, AuthManager>,
) -> Result<(), StructuredError> {
    manager
        .clear_license(&app)
        .await
        .map_err(StructuredError::from)
}
