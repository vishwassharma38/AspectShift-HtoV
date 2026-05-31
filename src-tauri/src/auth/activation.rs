#[cfg(feature = "dev-auth")]
use log::info;

#[cfg(feature = "dev-auth")]
use crate::auth::auth_errors::AuthError;
#[cfg(feature = "dev-auth")]
use crate::auth::providers::dev_provider::simulate_activation;
#[cfg(feature = "dev-auth")]
use crate::auth::validators::entitlement_validator::validate_license_key_format;

#[cfg(feature = "dev-auth")]
pub async fn activate_with_key(license_key: &str) -> Result<String, AuthError> {
    validate_license_key_format(license_key)?;

    info!(
        "Activation: attempting activation for key hint={}",
        &license_key[..4.min(license_key.len())]
    );

    let jwt = simulate_activation(license_key)?;

    info!("Activation: success");
    Ok(jwt)
}
