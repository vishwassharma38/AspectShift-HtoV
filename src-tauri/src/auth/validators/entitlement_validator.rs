use crate::auth::auth_errors::AuthError;

const MIN_KEY_LENGTH: usize = 10;
const DEV_KEY_ENV_VAR: &str = "ASPECTSHIFT_DEV_KEY";

pub fn validate_license_key_format(key: &str) -> Result<(), AuthError> {
    let trimmed = key.trim();

    if trimmed.is_empty() {
        return Err(AuthError::InvalidLicenseKey);
    }

    if trimmed.len() < MIN_KEY_LENGTH {
        return Err(AuthError::InvalidLicenseKey);
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AuthError::InvalidLicenseKey);
    }

    Ok(())
}

pub fn is_dev_key(key: &str) -> bool {
    let incoming = key.trim();
    if incoming.is_empty() {
        return false;
    }

    std::env::var(DEV_KEY_ENV_VAR)
        .ok()
        .map(|configured| incoming == configured.trim())
        .unwrap_or(false)
}

pub fn extract_token_hint(key: &str) -> String {
    let upper = key.trim().to_uppercase();
    upper[..8.min(upper.len())].to_string()
}

