use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::{info, warn};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tokio::sync::RwLock;

use crate::auth::auth_errors::{is_transient_refresh_failure, AuthError};
use crate::auth::auth_events::{
    emit_activation_failed, emit_activation_success, emit_auth_state, emit_license_invalid,
    emit_refresh_required,
};
use crate::auth::auth_models::{
    ActivationResult, UpdateEntitlementCheckResult, UpdateEntitlementCheckStatus,
};
use crate::auth::config::auth_config::app_version;
use crate::auth::machine::machine_id::get_machine_id;
use crate::auth::outcome_mapping::map_auth_error;
use crate::auth::providers::r#trait::LicenseProvider;
use crate::auth::state::auth_metadata::{AuthConfigMetadata, AuthPersistenceEnvelope, JwtMetadata};
use crate::auth::state::auth_state::{AuthState, AuthStatus};
use crate::auth::state::license_tier::LicenseTier;
use crate::auth::storage::secure_storage::{
    clear_all_credentials, delete_jwt, delete_license_key, load_jwt, load_license_key, store_jwt,
    store_license_key,
};
use crate::auth::validators::entitlement_validator::{
    extract_token_hint, validate_license_key_format,
};
use crate::auth::validators::jwt_validator::{classify_launch_status, validate_jwt};
use crate::auth::validators::launch_validation::run_launch_validation;
use crate::runtime_paths::RuntimePaths;

const AUTH_METADATA_FILENAME: &str = "auth_metadata.json";
const AUTH_METADATA_TMP_EXTENSION: &str = "tmp";

const RETRY_INTERVALS_MINS: &[u64] = &[15, 30, 60, 120, 240, 360];

struct ActivationRollback {
    jwt: Option<String>,
    license_key: Option<String>,
}

impl ActivationRollback {
    fn new(jwt: Option<String>, license_key: Option<String>) -> Self {
        Self { jwt, license_key }
    }
}

#[derive(Clone)]
pub struct AuthManager {
    provider: Arc<dyn LicenseProvider>,
    auth_status: Arc<RwLock<AuthStatus>>,
    license_tier: Arc<RwLock<LicenseTier>>,
    machine_id: Arc<RwLock<String>>,
    jwt_metadata: Arc<RwLock<Option<JwtMetadata>>>,
    auth_config: Arc<RwLock<AuthConfigMetadata>>,
    retry_index: Arc<RwLock<usize>>,
    refresh_in_progress: Arc<RwLock<bool>>,
    next_retry_at: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl AuthManager {
    pub fn new(provider: Arc<dyn LicenseProvider>) -> Self {
        Self {
            provider,
            auth_status: Arc::new(RwLock::new(AuthStatus::NotActivated)),
            license_tier: Arc::new(RwLock::new(LicenseTier::default())),
            machine_id: Arc::new(RwLock::new(String::new())),
            jwt_metadata: Arc::new(RwLock::new(None)),
            auth_config: Arc::new(RwLock::new(AuthConfigMetadata::default())),
            retry_index: Arc::new(RwLock::new(0)),
            refresh_in_progress: Arc::new(RwLock::new(false)),
            next_retry_at: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_auth_state(&self) -> AuthState {
        let status = self.auth_status.read().await.clone();
        let tier = self.license_tier.read().await.clone();
        let runtime_machine_id = self.machine_id.read().await.clone();
        let jwt_meta = self.jwt_metadata.read().await.clone();
        let config = self.auth_config.read().await.clone();

        let jwt_expires_at = config.jwt_expires_at.clone().or_else(|| {
            jwt_meta.as_ref().and_then(|meta| {
                DateTime::from_timestamp(meta.expires_at, 0).map(|d: DateTime<Utc>| d.to_rfc3339())
            })
        });

        let grace_expires_at = config.grace_expires_at.clone().or_else(|| {
            jwt_meta.as_ref().and_then(|meta| {
                DateTime::from_timestamp(meta.grace_expires_at, 0)
                    .map(|d: DateTime<Utc>| d.to_rfc3339())
            })
        });

        AuthState {
            status,
            tier,
            activated_at: config.activated_at.clone(),
            jwt_expires_at,
            grace_expires_at,
            token_hint: config.token_hint.clone(),
            machine_id: config.machine_id.clone().or_else(|| {
                if runtime_machine_id.is_empty() {
                    None
                } else {
                    Some(runtime_machine_id)
                }
            }),
        }
    }

    pub async fn run_launch_validation(&self, app: &AppHandle) {
        info!("AuthManager: starting launch validation");

        self.set_status(AuthStatus::Activating).await;
        self.load_auth_metadata(app).await;

        match self.ensure_machine_id().await {
            Ok(mid) => {
                let mut machine_id = self.machine_id.write().await;
                *machine_id = mid;
            }
            Err(e) => {
                warn!("AuthManager: failed to get machine ID at startup: {}", e);
            }
        }

        self.emit_current_state(app).await;

        // Keep this in sync with the HTTP request timeout default used by the license provider.
        let validation_timeout = tokio::time::Duration::from_secs(12);
        let result = tokio::time::timeout(validation_timeout, run_launch_validation()).await;

        match result {
            Ok(res) => {
                self.apply_validation_result(&res).await;
                if let Err(e) = self.recover_missing_metadata(&res, app).await {
                    warn!(
                        "AuthManager: could not recover missing auth metadata: {}",
                        e
                    );
                }
            }
            Err(_) => {
                warn!(
                    "AuthManager: launch validation timed out after {}s",
                    validation_timeout.as_secs()
                );
                let current_status = self.auth_status.read().await.clone();
                if current_status == AuthStatus::Activating {
                    self.set_status(AuthStatus::NotActivated).await;
                }
            }
        }

        let state = self.get_auth_state().await;
        self.emit_state_for_status(app, &state).await;

        if state.status.needs_refresh() {
            info!(
                "AuthManager: scheduling silent refresh after launch status={:?}",
                state.status
            );
            let manager = self.clone();
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = manager.silent_refresh(&app_handle).await {
                    warn!("AuthManager: silent refresh finished with error: {}", e);
                }
            });
        }
    }

    pub async fn activate(
        &self,
        app: &AppHandle,
        license_key: &str,
    ) -> Result<ActivationResult, AuthError> {
        info!("AuthManager: starting activation");

        let rollback =
            ActivationRollback::new(load_jwt().ok().flatten(), load_license_key().ok().flatten());

        if let Err(e) = validate_license_key_format(license_key) {
            warn!("AuthManager: activation failed: {}", e);
            return Err(e);
        }

        self.set_status(AuthStatus::Activating).await;
        self.emit_current_state(app).await;

        let machine_id = match self.ensure_machine_id().await {
            Ok(mid) => mid,
            Err(e) => {
                self.apply_error_state(&e).await;
                let state = self.get_auth_state().await;
                self.emit_state_for_status(app, &state).await;
                return Err(e);
            }
        };

        match self.provider.activate(license_key).await {
            Ok(jwt) => {
                let meta = match validate_jwt(&jwt) {
                    Ok(meta) => meta,
                    Err(e) => {
                        warn!("AuthManager: activation JWT validation failed: {}", e);
                        self.apply_error_state(&e).await;
                        let state = self.get_auth_state().await;
                        self.emit_state_for_status(app, &state).await;
                        let mapped = map_auth_error(&e);
                        emit_activation_failed(app, &e.to_string(), mapped.error_code);
                        return Err(e);
                    }
                };

                if meta.mid != machine_id {
                    let mismatch = AuthError::MachineMismatch;
                    self.apply_error_state(&mismatch).await;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    emit_activation_failed(app, &mismatch.to_string(), "machine_mismatch");
                    return Err(mismatch);
                }

                if classify_launch_status(&meta, Utc::now().timestamp()) == AuthStatus::Expired {
                    let expired = AuthError::LicenseExpired;
                    self.apply_error_state(&expired).await;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    emit_activation_failed(
                        app,
                        &expired.to_string(),
                        map_auth_error(&expired).error_code,
                    );
                    return Err(expired);
                }

                if let Err(e) = store_license_key(license_key) {
                    if let Err(rollback_err) = restore_license_key(&rollback.license_key) {
                        warn!(
                            "AuthManager: failed to restore previous license key: {}",
                            rollback_err
                        );
                    }
                    self.apply_error_state(&e).await;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    emit_activation_failed(app, &e.to_string(), map_auth_error(&e).error_code);
                    return Err(e);
                }
                if let Err(e) = store_jwt(&jwt) {
                    if let Err(rollback_err) = restore_jwt(&rollback.jwt) {
                        warn!("AuthManager: failed to restore previous JWT after activation error: {}", rollback_err);
                    }
                    if let Err(rollback_err) = restore_license_key(&rollback.license_key) {
                        warn!("AuthManager: failed to restore previous license key after activation error: {}", rollback_err);
                    }
                    self.apply_error_state(&e).await;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    emit_activation_failed(app, &e.to_string(), map_auth_error(&e).error_code);
                    return Err(e);
                }

                if let Err(e) = self
                    .save_auth_metadata(
                        app,
                        Some(&meta),
                        Some(extract_token_hint(license_key)),
                        None,
                        None,
                        None,
                    )
                    .await
                {
                    if let Err(rollback_err) = restore_jwt(&rollback.jwt) {
                        warn!(
                            "AuthManager: failed to restore previous JWT after metadata error: {}",
                            rollback_err
                        );
                    }
                    if let Err(rollback_err) = restore_license_key(&rollback.license_key) {
                        warn!("AuthManager: failed to restore previous license key after metadata error: {}", rollback_err);
                    }
                    self.apply_error_state(&e).await;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    emit_activation_failed(app, &e.to_string(), map_auth_error(&e).error_code);
                    return Err(e);
                }

                self.apply_validated_metadata(Some(meta.clone())).await;
                self.set_status(AuthStatus::Valid).await;
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
                self.apply_error_state(&e).await;
                let state = self.get_auth_state().await;
                self.emit_state_for_status(app, &state).await;
                let mapped = map_auth_error(&e);
                emit_activation_failed(app, &e.to_string(), mapped.error_code);
                warn!("AuthManager: activation failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn refresh(&self, app: &AppHandle) -> Result<AuthState, AuthError> {
        self.refresh_internal(app, false).await
    }

    pub async fn check_update_entitlement(
        &self,
        app: &AppHandle,
    ) -> UpdateEntitlementCheckResult {
        let current_state = self.get_auth_state().await;

        if matches!(current_state.status, AuthStatus::OfflineValid) {
            return UpdateEntitlementCheckResult::offline();
        }

        if matches!(
            current_state.status,
            AuthStatus::NotActivated
                | AuthStatus::Activating
                | AuthStatus::Invalid
                | AuthStatus::Expired
                | AuthStatus::MachineMismatch
                | AuthStatus::Corrupted
        ) {
            return UpdateEntitlementCheckResult::auth_required();
        }

        let mut status = current_state.status.clone();

        if matches!(status, AuthStatus::GracePeriod | AuthStatus::RefreshRequired) {
            match self.silent_refresh(app).await {
                Ok(refreshed_state) => {
                    status = refreshed_state.status;
                }
                Err(e) => {
                    if is_transient_refresh_failure(&e) {
                        return UpdateEntitlementCheckResult::offline();
                    }

                    return self.result_from_refresh_failure(&e);
                }
            }
        }

        if !status.allows_access() {
            return UpdateEntitlementCheckResult::auth_required();
        }

        let current_version = app_version();
        let mut retry_after_refresh = true;
        let mut token = match load_jwt() {
            Ok(Some(jwt)) => jwt,
            Ok(None) => return UpdateEntitlementCheckResult::auth_required(),
            Err(_) => return UpdateEntitlementCheckResult::server_error(),
        };

        loop {
            match self.provider.check_updates(&token, current_version).await {
                Ok(result) => {
                    info!(
                        "AuthManager: update entitlement response status={:?}",
                        result.status
                    );
                    match result.status {
                        UpdateEntitlementCheckStatus::UpdateAvailable => return result,
                        UpdateEntitlementCheckStatus::NoUpdate => {
                            return UpdateEntitlementCheckResult::no_update();
                        }
                        UpdateEntitlementCheckStatus::NotEntitled => {
                            return UpdateEntitlementCheckResult::not_entitled();
                        }
                        UpdateEntitlementCheckStatus::ChannelNotAllowed => {
                            return UpdateEntitlementCheckResult::channel_not_allowed();
                        }
                        UpdateEntitlementCheckStatus::Offline => {
                            return UpdateEntitlementCheckResult::offline();
                        }
                        UpdateEntitlementCheckStatus::ServerError => {
                            return UpdateEntitlementCheckResult::server_error();
                        }
                        UpdateEntitlementCheckStatus::AuthRequired => {
                            return UpdateEntitlementCheckResult::auth_required();
                        }
                    }
                }
                Err(AuthError::TokenCorrupted) if retry_after_refresh && status.allows_access() => {
                    retry_after_refresh = false;
                    match self.silent_refresh(app).await {
                        Ok(refreshed_state) => {
                            status = refreshed_state.status;
                            match load_jwt() {
                                Ok(Some(jwt)) => {
                                    token = jwt;
                                    continue;
                                }
                                Ok(None) => {
                                    return UpdateEntitlementCheckResult::auth_required();
                                }
                                Err(_) => {
                                    return UpdateEntitlementCheckResult::server_error();
                                }
                            }
                        }
                        Err(e) => {
                            if is_transient_refresh_failure(&e) {
                                return UpdateEntitlementCheckResult::offline();
                            }

                            return self.result_from_refresh_failure(&e);
                        }
                    }
                }
                Err(AuthError::TokenCorrupted) => {
                    return UpdateEntitlementCheckResult::auth_required();
                }
                Err(AuthError::LicenseRevoked) => {
                    self.apply_error_state(&AuthError::LicenseRevoked).await;
                    return UpdateEntitlementCheckResult::auth_required();
                }
                Err(AuthError::LicenseRefunded) => {
                    self.apply_error_state(&AuthError::LicenseRefunded).await;
                    return UpdateEntitlementCheckResult::auth_required();
                }
                Err(AuthError::MachineMismatch) => {
                    self.apply_error_state(&AuthError::MachineMismatch).await;
                    return UpdateEntitlementCheckResult::auth_required();
                }
                Err(AuthError::LicenseExpired) => {
                    self.apply_error_state(&AuthError::LicenseExpired).await;
                    return UpdateEntitlementCheckResult::auth_required();
                }
                Err(AuthError::NotActivated) => {
                    self.apply_error_state(&AuthError::NotActivated).await;
                    return UpdateEntitlementCheckResult::auth_required();
                }
                Err(AuthError::InvalidRequest) => {
                    return UpdateEntitlementCheckResult::server_error();
                }
                Err(AuthError::ServerError) => {
                    return UpdateEntitlementCheckResult::server_error();
                }
                Err(err) => {
                    warn!("AuthManager: update entitlement check failed: {}", err);
                    return UpdateEntitlementCheckResult::server_error();
                }
            }
        }
    }

    async fn silent_refresh(&self, app: &AppHandle) -> Result<AuthState, AuthError> {
        self.refresh_internal(app, true).await
    }

    async fn refresh_internal(
        &self,
        app: &AppHandle,
        silent: bool,
    ) -> Result<AuthState, AuthError> {
        {
            let mut in_progress = self.refresh_in_progress.write().await;
            if *in_progress {
                info!("AuthManager: refresh already in progress, skipping duplicate request");
                return Ok(self.get_auth_state().await);
            }
            *in_progress = true;
        }

        let result = self.refresh_execution(app, silent).await;

        {
            let mut in_progress = self.refresh_in_progress.write().await;
            *in_progress = false;
        }

        result
    }

    async fn refresh_execution(&self, app: &AppHandle, silent: bool) -> Result<AuthState, AuthError> {
        info!(
            "AuthManager: refresh execution started mode={}",
            if silent { "silent" } else { "manual" }
        );

        let previous_state = self.get_auth_state().await;
        let rollback_config = self.auth_config.read().await.clone();
        let token = load_jwt()?.ok_or(AuthError::NotActivated)?;
        let rollback_jwt = token.clone();

        if !silent {
            self.set_status(AuthStatus::Activating).await;
            self.emit_current_state(app).await;
        }

        let now_ts = Utc::now().timestamp();
        let now_iso = Utc::now().to_rfc3339();

        match self.provider.refresh(&token).await {
            Ok(new_jwt) => {
                {
                    let mut next = self.next_retry_at.write().await;
                    *next = None;
                }

                let new_meta = match validate_jwt(&new_jwt) {
                    Ok(meta) => meta,
                    Err(e) => {
                        let err = AuthError::RefreshFailed {
                            reason: e.to_string(),
                        };
                        warn!("AuthManager: refresh JWT validation failed: {}", err);
                        self.apply_error_state(&err).await;
                        self.save_auth_metadata(
                            app,
                            None,
                            None,
                            None,
                            None,
                            Some((now_iso.clone(), Some(err.to_string()))),
                        )
                        .await?;
                        let state = self.get_auth_state().await;
                        self.emit_state_for_status(app, &state).await;
                        return Err(err);
                    }
                };
                info!("AuthManager: refresh JWT validated");

                let machine_id = match self.ensure_machine_id().await {
                    Ok(mid) => mid,
                    Err(e) => {
                        self.apply_error_state(&e).await;
                        let state = self.get_auth_state().await;
                        self.emit_state_for_status(app, &state).await;
                        return Err(e);
                    }
                };
                if new_meta.mid != machine_id {
                    let mismatch = AuthError::MachineMismatch;
                    self.apply_error_state(&mismatch).await;
                    self.save_auth_metadata(
                        app,
                        None,
                        None,
                        None,
                        None,
                        Some((now_iso.clone(), Some(mismatch.to_string()))),
                    )
                    .await?;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    return Err(mismatch);
                }

                let refreshed_status = classify_launch_status(&new_meta, now_ts);
                if refreshed_status == AuthStatus::Expired {
                    let expired = AuthError::LicenseExpired;
                    self.apply_error_state(&expired).await;
                    self.save_auth_metadata(
                        app,
                        None,
                        None,
                        None,
                        None,
                        Some((now_iso.clone(), Some(expired.to_string()))),
                    )
                    .await?;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    return Err(expired);
                }

                if let Err(e) = store_jwt(&new_jwt) {
                    warn!("AuthManager: refresh rollback triggered");
                    if let Err(rollback_err) = restore_jwt(&Some(rollback_jwt.clone())) {
                        warn!("AuthManager: failed to restore previous JWT after refresh write error: {}", rollback_err);
                    }
                    self.apply_error_state(&e).await;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    return Err(e);
                }

                if let Err(e) = self
                    .save_auth_metadata(
                        app,
                        Some(&new_meta),
                        None,
                        Some(now_iso.clone()),
                        None,
                        Some((now_iso.clone(), None)),
                    )
                    .await
                {
                    warn!("AuthManager: refresh rollback triggered");
                    if let Err(rollback_err) = restore_jwt(&Some(rollback_jwt.clone())) {
                        warn!(
                            "AuthManager: failed to restore previous JWT after refresh error: {}",
                            rollback_err
                        );
                    }
                    if let Err(rollback_err) = self.restore_auth_metadata(app, &rollback_config) {
                        warn!(
                            "AuthManager: failed to restore previous metadata after refresh error: {}",
                            rollback_err
                        );
                    }
                    {
                        let mut config = self.auth_config.write().await;
                        *config = rollback_config;
                    }
                    self.set_status(previous_state.status).await;
                    let state = self.get_auth_state().await;
                    self.emit_state_for_status(app, &state).await;
                    return Err(e);
                }

                {
                    let mut idx = self.retry_index.write().await;
                    *idx = 0;
                }

                self.apply_validated_metadata(Some(new_meta)).await;
                self.set_status(refreshed_status).await;

                let state = self.get_auth_state().await;
                emit_auth_state(app, &state);
                info!("AuthManager: refresh persistence completed");
                info!("AuthManager: refresh successful");
                Ok(state)
            }
            Err(e) => {
                if is_transient_refresh_failure(&e) {
                    warn!(
                        "AuthManager: refresh failed transiently, retaining existing auth state: {}",
                        e
                    );

                    let mut next_status = previous_state.status;
                    let mut grace_start = rollback_config.grace_started_at.clone();

                    if let Some(meta) = self.jwt_metadata.read().await.as_ref() {
                        let potential_status = classify_launch_status(meta, now_ts);
                        if potential_status == AuthStatus::GracePeriod {
                            next_status = AuthStatus::GracePeriod;
                            if grace_start.is_none() {
                                grace_start = Some(now_iso.clone());
                            }
                        }
                    }

                    self.set_status(next_status).await;

                    if let Err(save_err) = self
                        .save_auth_metadata(
                            app,
                            None,
                            None,
                            None,
                            grace_start,
                            Some((now_iso, Some(e.to_string()))),
                        )
                        .await
                    {
                        warn!("AuthManager: failed to save transient failure metadata: {}", save_err);
                    }

                    self.schedule_retry(app).await;

                    let retained_state = self.get_auth_state().await;
                    if !silent {
                        self.emit_state_for_status(app, &retained_state).await;
                    }
                    return Err(e);
                }

                self.apply_error_state(&e).await;
                if let Err(save_err) = self
                    .save_auth_metadata(
                        app,
                        None,
                        None,
                        None,
                        None,
                        Some((now_iso, Some(e.to_string()))),
                    )
                    .await
                {
                    warn!("AuthManager: failed to save authoritative failure metadata: {}", save_err);
                }
                let state = self.get_auth_state().await;
                self.emit_state_for_status(app, &state).await;
                warn!("AuthManager: refresh failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn schedule_retry(&self, app: &AppHandle) {
        {
            let next = self.next_retry_at.read().await;
            if next.is_some() {
                info!("AuthManager: retry already scheduled, skipping duplicate");
                return;
            }
        }

        let index = {
            let idx = self.retry_index.read().await;
            *idx
        };

        let mins = if index >= RETRY_INTERVALS_MINS.len() {
            RETRY_INTERVALS_MINS[RETRY_INTERVALS_MINS.len() - 1]
        } else {
            let m = RETRY_INTERVALS_MINS[index];
            let mut idx = self.retry_index.write().await;
            *idx += 1;
            m
        };

        {
            let mut next = self.next_retry_at.write().await;
            *next = Some(Utc::now() + chrono::Duration::minutes(mins as i64));
        }

        self.start_retry_timer(app, mins);
    }

    pub async fn trigger_reactive_refresh(&self, app: &AppHandle) {
        let status = {
            let s = self.auth_status.read().await;
            s.clone()
        };

        if status == AuthStatus::GracePeriod || status == AuthStatus::RefreshRequired {
            {
                let next = self.next_retry_at.read().await;
                if let Some(at) = *next {
                    if Utc::now() < at {
                        info!("AuthManager: reactive refresh skipped, cooldown active until {}", at);
                        return;
                    }
                }
            }

            info!(
                "AuthManager: reactive refresh triggered by lifecycle event status={:?}",
                status
            );
            let _ = self.silent_refresh(app).await;
        }
    }

    fn start_retry_timer(&self, app: &AppHandle, mins: u64) {
        info!(
            "AuthManager: scheduling next refresh retry in {} minutes",
            mins
        );
        let manager = self.clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(mins * 60)).await;

            // Clear the pending status before attempting
            {
                let mut next = manager.next_retry_at.write().await;
                *next = None;
            }

            // Guard against stale refreshes if recovery already occurred
            if !manager.get_auth_state().await.status.needs_refresh() {
                info!("AuthManager: scheduled retry skipped, license already recovered");
                return;
            }

            if let Err(e) = manager.silent_refresh(&app_handle).await {
                warn!("AuthManager: scheduled retry finished with error: {}", e);
            }
        });
    }

    pub async fn clear_license(&self, app: &AppHandle) -> Result<(), AuthError> {
        clear_all_credentials()?;

        self.set_status(AuthStatus::NotActivated).await;
        {
            let mut tier = self.license_tier.write().await;
            *tier = LicenseTier::default();
        }
        {
            let mut jwt_metadata = self.jwt_metadata.write().await;
            jwt_metadata.take();
        }
        {
            let mut config = self.auth_config.write().await;
            *config = AuthConfigMetadata::default();
        }
        {
            let mut idx = self.retry_index.write().await;
            *idx = 0;
        }

        if let Err(e) = self.delete_auth_metadata(app).await {
            warn!(
                "AuthManager: could not delete auth metadata file, rewriting default metadata: {}",
                e
            );
            if let Err(write_err) = self
                .write_auth_metadata(app, &AuthConfigMetadata::default())
                .await
            {
                warn!(
                    "AuthManager: could not rewrite default auth metadata after delete failure: {}",
                    write_err
                );
            }
        }

        let state = self.get_auth_state().await;
        emit_auth_state(app, &state);

        info!("AuthManager: license cleared");
        Ok(())
    }

    async fn apply_validation_result(
        &self,
        result: &crate::auth::validators::launch_validation::LaunchValidationResult,
    ) {
        self.set_status(result.status.clone()).await;

        if let Some(metadata) = result.jwt_metadata.clone() {
            self.apply_validated_metadata(Some(metadata)).await;
        } else {
            self.apply_validated_metadata(None).await;
        }
    }

    async fn recover_missing_metadata(
        &self,
        result: &crate::auth::validators::launch_validation::LaunchValidationResult,
        app: &AppHandle,
    ) -> Result<(), AuthError> {
        if !should_recover_missing_metadata(result) {
            return Ok(());
        }

        let needs_recovery = {
            let config = self.auth_config.read().await;
            config.eq(&AuthConfigMetadata::default())
        };

        if !needs_recovery {
            return Ok(());
        }

        if let Some(metadata) = result.jwt_metadata.as_ref() {
            self.save_auth_metadata(app, Some(metadata), None, None, None, None)
                .await?;
            info!("AuthManager: recovered missing auth metadata from validated JWT");
        }

        Ok(())
    }

    async fn apply_validated_metadata(&self, metadata: Option<JwtMetadata>) {
        match metadata {
            Some(meta) => {
                {
                    let mut tier = self.license_tier.write().await;
                    *tier = meta.tier.clone();
                }
                {
                    let mut machine_id = self.machine_id.write().await;
                    *machine_id = meta.mid.clone();
                }
                {
                    let mut jwt_metadata = self.jwt_metadata.write().await;
                    *jwt_metadata = Some(meta);
                }
            }
            None => {
                {
                    let mut tier = self.license_tier.write().await;
                    *tier = LicenseTier::default();
                }
                {
                    let mut jwt_metadata = self.jwt_metadata.write().await;
                    *jwt_metadata = None;
                }
            }
        }
    }

    async fn apply_error_state(&self, error: &AuthError) {
        let mapped = map_auth_error(error);
        self.set_status(mapped.status).await;
    }

    fn result_from_refresh_failure(&self, error: &AuthError) -> UpdateEntitlementCheckResult {
        match error {
            AuthError::ServerError
            | AuthError::InvalidRequest
            | AuthError::StorageError(_)
            | AuthError::MachineIdError(_)
            | AuthError::JsonError(_)
            | AuthError::TauriError(_) => UpdateEntitlementCheckResult::server_error(),
            _ => UpdateEntitlementCheckResult::auth_required(),
        }
    }

    async fn set_status(&self, status: AuthStatus) {
        let mut guard = self.auth_status.write().await;
        *guard = status;
    }

    async fn ensure_machine_id(&self) -> Result<String, AuthError> {
        let current = self.machine_id.read().await.clone();
        if !current.is_empty() {
            return Ok(current);
        }

        let mid_res = tokio::task::spawn_blocking(|| get_machine_id()).await;
        let machine_id = match mid_res {
            Ok(res) => res?,
            Err(e) => return Err(AuthError::MachineIdError(e.to_string())),
        };

        let mut guard = self.machine_id.write().await;
        *guard = machine_id.clone();
        Ok(machine_id)
    }

    async fn emit_current_state(&self, app: &AppHandle) {
        let state = self.get_auth_state().await;
        self.emit_state_for_status(app, &state).await;
    }

    async fn emit_state_for_status(&self, app: &AppHandle, state: &AuthState) {
        match state.status {
            AuthStatus::RefreshRequired | AuthStatus::GracePeriod => {
                emit_refresh_required(app, state);
            }
            AuthStatus::Expired
            | AuthStatus::Invalid
            | AuthStatus::Corrupted
            | AuthStatus::MachineMismatch => {
                emit_license_invalid(app, state);
            }
            _ => emit_auth_state(app, state),
        }
    }

    async fn save_auth_metadata(
        &self,
        app: &AppHandle,
        metadata: Option<&JwtMetadata>,
        token_hint: Option<String>,
        last_refresh_success_at: Option<String>,
        grace_started_at: Option<String>,
        last_refresh_attempt: Option<(String, Option<String>)>,
    ) -> Result<(), AuthError> {
        let current_config = self.auth_config.read().await.clone();
        let runtime_machine_id = self.machine_id.read().await.clone();

        let activated_at = resolve_activated_at(metadata, &current_config);
        let jwt_expires_at = metadata
            .and_then(|meta| {
                DateTime::from_timestamp(meta.expires_at, 0).map(|d: DateTime<Utc>| d.to_rfc3339())
            })
            .or(current_config.jwt_expires_at.clone());

        let grace_expires_at = metadata
            .and_then(|meta| {
                DateTime::from_timestamp(meta.grace_expires_at, 0)
                    .map(|d: DateTime<Utc>| d.to_rfc3339())
            })
            .or(current_config.grace_expires_at.clone());

        let update_expires_at = metadata
            .and_then(|meta| {
                DateTime::from_timestamp(meta.update_expires_at, 0)
                    .map(|d: DateTime<Utc>| d.to_rfc3339())
            })
            .or(current_config.update_expires_at.clone());

        let build_channel = metadata
            .map(|meta| meta.channel.clone())
            .or(current_config.build_channel.clone());

        let now = Utc::now().to_rfc3339();

        let metadata = AuthConfigMetadata {
            machine_id: if runtime_machine_id.is_empty() {
                current_config.machine_id.clone()
            } else {
                Some(runtime_machine_id)
            },
            activated_at: Some(activated_at),
            jwt_expires_at,
            grace_expires_at,
            update_expires_at,
            last_refresh_at: last_refresh_success_at
                .clone()
                .or(current_config.last_refresh_at.clone()),
            last_validation_at: Some(now),
            token_hint: token_hint.or(current_config.token_hint.clone()),
            build_channel,
            purchase_token_hint: current_config.purchase_token_hint.clone(),
            grace_started_at: grace_started_at.or(current_config.grace_started_at.clone()),
            last_refresh_attempt_at: last_refresh_attempt
                .as_ref()
                .map(|(at, _)| at.clone())
                .or(current_config.last_refresh_attempt_at.clone()),
            last_refresh_success_at: last_refresh_success_at
                .or(current_config.last_refresh_success_at.clone()),
            last_refresh_failure_reason: last_refresh_attempt
                .as_ref()
                .and_then(|(_, reason)| reason.clone())
                .or(current_config.last_refresh_failure_reason.clone()),
        };

        self.write_auth_metadata(app, &metadata).await?;
        let mut config = self.auth_config.write().await;
        *config = metadata;
        Ok(())
    }

    async fn load_auth_metadata(&self, app: &AppHandle) {
        let path = match self.metadata_path(app) {
            Ok(path) => path,
            Err(e) => {
                warn!("AuthManager: could not resolve auth metadata path: {}", e);
                return;
            }
        };
        if !path.exists() {
            return;
        }

        match read_auth_metadata_file(&path) {
            Ok(metadata) => {
                let mut config = self.auth_config.write().await;
                *config = metadata;
            }
            Err(e) => {
                warn!("AuthManager: could not load persisted metadata: {}", e);
            }
        }
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

    async fn write_auth_metadata(
        &self,
        app: &AppHandle,
        metadata: &AuthConfigMetadata,
    ) -> Result<(), AuthError> {
        let path = self.metadata_path(app)?;
        write_auth_metadata_file(&path, metadata)
    }

    fn restore_auth_metadata(
        &self,
        app: &AppHandle,
        metadata: &AuthConfigMetadata,
    ) -> Result<(), AuthError> {
        let path = self.metadata_path(app)?;
        write_auth_metadata_file(&path, metadata)
    }
}

fn auth_metadata_tmp_path(path: &Path) -> PathBuf {
    path.with_extension(AUTH_METADATA_TMP_EXTENSION)
}

fn auth_metadata_envelope_from_content(content: &str) -> Result<AuthConfigMetadata, AuthError> {
    if let Ok(envelope) = serde_json::from_str::<AuthPersistenceEnvelope>(content) {
        if envelope.schema_version != AuthPersistenceEnvelope::SCHEMA_VERSION {
            return Err(AuthError::StorageError(format!(
                "Unsupported auth metadata schema version: {}",
                envelope.schema_version
            )));
        }
        return Ok(envelope.auth);
    }

    let metadata: AuthConfigMetadata = serde_json::from_str(content)?;
    Ok(metadata)
}

fn read_auth_metadata_file(path: &Path) -> Result<AuthConfigMetadata, AuthError> {
    let content = fs::read_to_string(path)?;
    auth_metadata_envelope_from_content(&content)
}

fn write_auth_metadata_file(path: &Path, metadata: &AuthConfigMetadata) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let envelope = AuthPersistenceEnvelope::new(metadata.clone());
    let serialized = serde_json::to_string_pretty(&envelope)?;
    let tmp_path = auth_metadata_tmp_path(path);

    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(serialized.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
    }

    let written = fs::read_to_string(&tmp_path)?;
    let validated = auth_metadata_envelope_from_content(&written)?;
    if &validated != metadata {
        let _ = fs::remove_file(&tmp_path);
        return Err(AuthError::StorageError(
            "Auth metadata validation failed after write".to_string(),
        ));
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn should_recover_missing_metadata(
    result: &crate::auth::validators::launch_validation::LaunchValidationResult,
) -> bool {
    matches!(
        &result.status,
        &AuthStatus::Valid
            | &AuthStatus::GracePeriod
            | &AuthStatus::RefreshRequired
            | &AuthStatus::Expired
    ) && result.jwt_metadata.is_some()
}

fn restore_jwt(jwt: &Option<String>) -> Result<(), AuthError> {
    match jwt {
        Some(value) => store_jwt(value),
        None => delete_jwt(),
    }
}

fn restore_license_key(key: &Option<String>) -> Result<(), AuthError> {
    match key {
        Some(value) => store_license_key(value),
        None => delete_license_key(),
    }
}

fn resolve_activated_at(
    metadata: Option<&JwtMetadata>,
    current_config: &AuthConfigMetadata,
) -> String {
    if let Some(activated_at) = current_config.activated_at.clone() {
        info!("AuthManager: recovered activation timestamp from persisted metadata");
        return activated_at;
    }

    if let Some(meta) = metadata {
        if let Some(ts) = DateTime::from_timestamp(meta.issued_at, 0) {
            let activated_at = ts.to_rfc3339();
            info!("AuthManager: recovered activation timestamp from JWT claim");
            return activated_at;
        }
    }

    info!("AuthManager: activation timestamp unavailable, using current time");
    Utc::now().to_rfc3339()
}
