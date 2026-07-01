#[cfg(feature = "dev-auth")]
use log::info;

#[cfg(feature = "dev-auth")]
use crate::auth::auth_errors::AuthError;
#[cfg(feature = "dev-auth")]
use crate::auth::auth_models::UpdateEntitlementCheckResult;
#[cfg(feature = "dev-auth")]
use crate::auth::providers::r#trait::{
    ActivationResponse, EntitlementClaims, LicenseProvider, LicenseToken, RefreshResponse,
};
#[cfg(feature = "dev-auth")]
use crate::auth::state::auth_metadata::JwtMetadata;
#[cfg(feature = "dev-auth")]
use crate::auth::state::auth_state::AuthStatus;
#[cfg(feature = "dev-auth")]
use crate::auth::state::license_tier::LicenseTier;
#[cfg(feature = "dev-auth")]
use crate::auth::validators::entitlement_validator::is_dev_key;

#[cfg(feature = "dev-auth")]
use crate::auth::machine::machine_id::get_machine_id;
#[cfg(feature = "dev-auth")]
use crate::auth::validators::jwt_validator::{generate_jwt, validate_jwt};

#[cfg(feature = "dev-auth")]
pub struct DevLicenseProvider;

#[cfg(feature = "dev-auth")]
impl DevLicenseProvider {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "dev-auth")]
impl LicenseProvider for DevLicenseProvider {
    fn activate<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ActivationResponse, AuthError>> + Send + 'a>,
    > {
        Box::pin(async move { simulate_activation(key) })
    }

    fn refresh<'a>(
        &'a self,
        token: &'a LicenseToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<RefreshResponse, AuthError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let existing_metadata = validate_jwt(token)?;
            simulate_refresh(&existing_metadata)
        })
    }

    fn check_updates<'a>(
        &'a self,
        _token: &'a LicenseToken,
        _current_version: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<UpdateEntitlementCheckResult, AuthError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Ok(UpdateEntitlementCheckResult::no_update()) })
    }

    fn validate<'a>(
        &'a self,
        token: &'a LicenseToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<EntitlementClaims, AuthError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let metadata = validate_jwt(token)?;
            if metadata.is_expired() {
                return Ok(AuthStatus::Expired);
            }
            Ok(AuthStatus::Valid)
        })
    }

    fn deactivate<'a>(
        &'a self,
        _token: &'a LicenseToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AuthError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
}

#[cfg(feature = "dev-auth")]
pub fn simulate_activation(license_key: &str) -> Result<String, AuthError> {
    if !is_dev_key(license_key) {
        info!("LocalSimulator: key does not match canonical dev key, rejecting");
        return Err(AuthError::InvalidLicenseKey);
    }

    let machine_id = get_machine_id()?;
    let tier = determine_tier_from_key(license_key);
    let sub = format!("sim_user_{}", &machine_id[4..].to_lowercase());

    info!(
        "LocalSimulator: activating for sub={} tier={} mid={}",
        sub,
        tier.as_str(),
        machine_id
    );

    let jwt = generate_jwt(&sub, &tier, &machine_id).map_err(|e| AuthError::ActivationFailed {
        reason: e.to_string(),
    })?;

    info!("LocalSimulator: activation complete");
    Ok(jwt)
}

#[cfg(feature = "dev-auth")]
pub fn simulate_refresh(existing_metadata: &JwtMetadata) -> Result<String, AuthError> {
    let jwt = generate_jwt(
        &existing_metadata.sub,
        &existing_metadata.tier,
        &existing_metadata.mid,
    )
    .map_err(|e| AuthError::RefreshFailed {
        reason: e.to_string(),
    })?;

    info!("LocalSimulator: refresh complete");
    Ok(jwt)
}

#[cfg(feature = "dev-auth")]
pub fn validate_local_jwt(jwt: &str) -> Result<JwtMetadata, AuthError> {
    validate_jwt(jwt)
}

#[cfg(feature = "dev-auth")]
fn determine_tier_from_key(key: &str) -> LicenseTier {
    let upper = key.to_uppercase();
    if upper.contains("DEV") || is_dev_key(key) {
        LicenseTier::Pro
    } else {
        LicenseTier::Community
    }
}
