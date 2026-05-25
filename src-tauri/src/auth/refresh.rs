#[cfg(feature = "dev-auth")]
use log::info;

#[cfg(feature = "dev-auth")]
use crate::auth::auth_errors::AuthError;
#[cfg(feature = "dev-auth")]
use crate::auth::providers::dev_provider::simulate_refresh;
#[cfg(feature = "dev-auth")]
use crate::auth::state::auth_metadata::JwtMetadata;

#[cfg(feature = "dev-auth")]
pub async fn refresh_jwt(existing_metadata: &JwtMetadata) -> Result<String, AuthError> {
    info!("Refresh: attempting JWT refresh for sub={}", existing_metadata.sub);

    let new_jwt = simulate_refresh(existing_metadata)?;

    info!("Refresh: success");
    Ok(new_jwt)
}

