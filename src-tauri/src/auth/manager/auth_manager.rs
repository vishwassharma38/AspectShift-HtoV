use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::{info, warn};
use tauri::AppHandle;
use tokio::sync::RwLock;

use crate::auth::auth_errors::AuthError;
use crate::auth::auth_events::{
    emit_activation_failed, emit_activation_success, emit_auth_state, emit_license_invalid,
    emit_refresh_required,
};
use crate::auth::auth_models::ActivationResult;
use crate::auth::providers::r#trait::LicenseProvider;
use crate::auth::state::auth_metadata::{AuthConfigMetadata, JwtMetadata};
use crate::auth::state::auth_state::{AuthState, AuthStatus};
use crate::auth::state::license_tier::LicenseTier;
use crate::auth::validators::entitlement_validator::{extract_token_hint, validate_license_key_format};
use crate::auth::validators::jwt_validator::validate_jwt;
use crate::auth::validators::launch_validation::run_launch_validation;
use crate::auth::machine::machine_id::get_machine_id;
use crate::auth::storage::secure_storage::clear_all_credentials;
use crate::runtime_paths::RuntimePaths;

const AUTH_METADATA_FILENAME: &str = "auth_metadata.json";

#[derive(Clone)]
pub struct AuthManager {
    provider: Arc<dyn LicenseProvider>,
    auth_status: Arc<RwLock<AuthStatus>>,
    license_tier: Arc<RwLock<LicenseTier>>,
    machine_id: Arc<RwLock<String>>,
    jwt_metadata: Arc<RwLock<Option<JwtMetadata>>>,
}

impl AuthManager {
    pub fn new(provider: Arc<dyn LicenseProvider>) -> Self {
        Self {
            provider,
            auth_status: Arc::new(RwLock::new(AuthStatus::NotActivated)),
            license_tier: Arc::new(RwLock::new(LicenseTier::default())),
            machine_id: Arc::new(RwLock::new(String::new())),
            jwt_metadata: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_auth_state(&self) -> AuthState {
        let status = self.auth_status.read().await.clone();
        let tier = self.license_tier.read().await.clone();
        let mid = self.machine_id.read().await.clone();
        let jwt_meta = self.jwt_metadata.read().await.clone();

        let (activated_at, jwt_expires_at, token_hint) = match &jwt_meta {
            Some(meta) => {
                let exp = DateTime::from_timestamp(meta.expires_at, 0)
                    .map(|d: DateTime<Utc>| d.to_rfc3339());
                (None, exp, None)
            }
            None => (None, None, None),
        };

        AuthState {
            status,
            tier,
            activated_at,
            jwt_expires_at,
            token_hint,
            machine_id: if mid.is_empty() { None } else { Some(mid) },
        }
    }

    pub async fn run_launch_validation(&self, app: &AppHandle) {
        info!("AuthManager: starting launch validation");

        {
            let mut status = self.auth_status.write().await;
            *status = AuthStatus::Activating;
        }

        match get_machine_id() {
            Ok(mid) => {
                let mut machine_id = self.machine_id.write().await;
                *machine_id = mid;
            }
            Err(e) => {
                warn!("AuthManager: failed to get machine ID at startup: {}", e);
            }
        }

        let result = run_launch_validation().await;
        let status_for_emit = result.status.clone();

        {
            let mut status = self.auth_status.write().await;
            *status = result.status;
        }
        {
            let mut tier = self.license_tier.write().await;
            *tier = result
                .jwt_metadata
                .as_ref()
                .map(|m| m.tier.clone())
                .unwrap_or_default();
        }
        {
            let mut meta = self.jwt_metadata.write().await;
            *meta = result.jwt_metadata;
        }

        if let Err(e) = self.load_auth_metadata(app).await {
            warn!("AuthManager: could not load persisted metadata: {}", e);
        }

        let state = self.get_auth_state().await;

        match status_for_emit {
            AuthStatus::Valid | AuthStatus::OfflineValid => {
                info!("AuthManager: launch validation complete - Valid");
                emit_auth_state(app, &state);
            }
            AuthStatus::RefreshRequired | AuthStatus::GracePeriod => {
                warn!("AuthManager: launch validation - refresh required");
                emit_refresh_required(app, &state);
            }
            AuthStatus::Expired
            | AuthStatus::Invalid
            | AuthStatus::Corrupted
            | AuthStatus::MachineMismatch => {
                warn!("AuthManager: launch validation - license invalid");
                emit_license_invalid(app, &state);
            }
            AuthStatus::NotActivated | AuthStatus::Activating => {
                info!("AuthManager: launch validation - not activated");
                emit_auth_state(app, &state);
            }
        }
    }

    pub async fn activate(
        &self,
        app: &AppHandle,
        license_key: &str,
    ) -> Result<ActivationResult, AuthError> {
        info!("AuthManager: starting activation");

        if let Err(e) = validate_license_key_format(license_key) {
            warn!("AuthManager: activation failed: {}", e);
            return Err(e);
        }

        {
            let mut status = self.auth_status.write().await;
            *status = AuthStatus::Activating;
        }
        let activating_state = self.get_auth_state().await;
        emit_auth_state(app, &activating_state);

        match self.provider.activate(license_key).await {
            Ok(jwt) => match validate_jwt(&jwt) {
                Ok(meta) => {
                    let tier = meta.tier.clone();
                    let mid = meta.mid.clone();

                    {
                        let mut status = self.auth_status.write().await;
                        *status = AuthStatus::Valid;
                    }
                    {
                        let mut t = self.license_tier.write().await;
                        *t = tier;
                    }
                    {
                        let mut m = self.machine_id.write().await;
                        *m = mid;
                    }
                    {
                        let mut jm = self.jwt_metadata.write().await;
                        *jm = Some(meta);
                    }

                    let hint = extract_token_hint(license_key);
                    if let Err(e) = self.save_auth_metadata(app, Some(hint)).await {
                        warn!("AuthManager: failed to save auth metadata: {}", e);
                    }

                    let state = self.get_auth_state().await;
                    emit_activation_success(app, &state);

                    info!("AuthManager: activation successful");
                    Ok(ActivationResult {
                        success: true,
                        auth_state: state,
                        message: Some("License activated successfully".to_string()),
                    })
                }
                Err(e) => {
                    self.set_invalid_state().await;
                    let state = self.get_auth_state().await;
                    emit_license_invalid(app, &state);
                    emit_activation_failed(app, &e.to_string(), "token_corrupted");
                    warn!("AuthManager: activation failed: {}", e);
                    Err(AuthError::TokenCorrupted)
                }
            },
            Err(e) => {
                self.set_invalid_state().await;
                let state = self.get_auth_state().await;
                emit_license_invalid(app, &state);
                emit_activation_failed(app, &e.to_string(), "activation_failed");
                warn!("AuthManager: activation failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn refresh(&self, app: &AppHandle) -> Result<AuthState, AuthError> {
        let existing_meta = {
            let meta = self.jwt_metadata.read().await;
            meta.clone()
        };

        let metadata = match existing_meta {
            Some(m) => m,
            None => return Err(AuthError::NotActivated),
        };

        let token = validate_jwt_to_token(&metadata)?;
        match self.provider.refresh(&token).await {
            Ok(new_jwt) => match validate_jwt(&new_jwt) {
                Ok(new_meta) => {
                    {
                        let mut status = self.auth_status.write().await;
                        *status = AuthStatus::Valid;
                    }
                    {
                        let mut t = self.license_tier.write().await;
                        *t = new_meta.tier.clone();
                    }
                    {
                        let mut jm = self.jwt_metadata.write().await;
                        *jm = Some(new_meta);
                    }

                    let state = self.get_auth_state().await;
                    emit_auth_state(app, &state);
                    info!("AuthManager: refresh successful");
                    Ok(state)
                }
                Err(e) => Err(AuthError::RefreshFailed {
                    reason: e.to_string(),
                }),
            },
            Err(e) => {
                warn!("AuthManager: refresh failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn clear_license(&self, app: &AppHandle) -> Result<(), AuthError> {
        clear_all_credentials()?;

        {
            let mut status = self.auth_status.write().await;
            *status = AuthStatus::NotActivated;
        }
        {
            let mut t = self.license_tier.write().await;
            *t = LicenseTier::default();
        }
        {
            let mut jm = self.jwt_metadata.write().await;
            *jm = None;
        }

        if let Err(e) = self.delete_auth_metadata(app).await {
            warn!("AuthManager: could not delete auth metadata file: {}", e);
        }

        let state = self.get_auth_state().await;
        emit_auth_state(app, &state);

        info!("AuthManager: license cleared");
        Ok(())
    }

    async fn set_invalid_state(&self) {
        let mut status = self.auth_status.write().await;
        *status = AuthStatus::Invalid;
    }

    async fn save_auth_metadata(
        &self,
        app: &AppHandle,
        token_hint: Option<String>,
    ) -> Result<(), AuthError> {
        let jwt_meta = self.jwt_metadata.read().await.clone();
        let machine_id = self.machine_id.read().await.clone();

        let jwt_expires_at = jwt_meta.as_ref().and_then(|m| {
            DateTime::from_timestamp(m.expires_at, 0).map(|d: DateTime<Utc>| d.to_rfc3339())
        });

        let metadata = AuthConfigMetadata {
            machine_id: if machine_id.is_empty() {
                None
            } else {
                Some(machine_id)
            },
            activated_at: Some(Utc::now().to_rfc3339()),
            jwt_expires_at,
            token_hint,
        };

        let path = self.metadata_path(app)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    async fn load_auth_metadata(&self, app: &AppHandle) -> Result<(), AuthError> {
        let path = self.metadata_path(app)?;
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(&path)?;
        let _metadata: AuthConfigMetadata = serde_json::from_str(&content)?;
        Ok(())
    }

    async fn delete_auth_metadata(&self, app: &AppHandle) -> Result<(), AuthError> {
        let path = self.metadata_path(app)?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn metadata_path(&self, app: &AppHandle) -> Result<std::path::PathBuf, AuthError> {
        let runtime =
            RuntimePaths::from_app(app).map_err(|e| AuthError::StorageError(e.to_string()))?;
        Ok(runtime.root().join(AUTH_METADATA_FILENAME))
    }
}

fn validate_jwt_to_token(_metadata: &JwtMetadata) -> Result<String, AuthError> {
    use crate::auth::storage::secure_storage::load_jwt;

    match load_jwt()? {
        Some(token) => Ok(token),
        None => Err(AuthError::NotActivated),
    }
}

