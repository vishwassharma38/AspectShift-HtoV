use keyring::Entry;
use log::{info, warn};

use crate::auth::auth_errors::AuthError;

const SERVICE_NAME: &str = "com.softwarefromvish.aspectshift-htov";
const JWT_KEY: &str = "auth_jwt";
const LICENSE_KEY: &str = "auth_license_key";

fn storage_backend_label() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        return "windows-keyring";
    }
    #[cfg(not(target_os = "windows"))]
    {
        return "platform-native-or-mock";
    }
}

fn get_entry(key: &str) -> Result<Entry, AuthError> {
    info!(
        "SecureStorage: creating entry key={} service={} backend={}",
        key,
        SERVICE_NAME,
        storage_backend_label()
    );
    Entry::new(SERVICE_NAME, key).map_err(|e| AuthError::StorageError(e.to_string()))
}

pub fn store_jwt(jwt: &str) -> Result<(), AuthError> {
    let entry = get_entry(JWT_KEY)?;
    entry
        .set_password(jwt)
        .map_err(|e| AuthError::StorageError(format!("Failed to store JWT: {}", e)))?;
    info!(
        "SecureStorage: stored JWT key={} backend={}",
        JWT_KEY,
        storage_backend_label()
    );
    Ok(())
}

pub fn load_jwt() -> Result<Option<String>, AuthError> {
    let entry = get_entry(JWT_KEY)?;
    match entry.get_password() {
        Ok(jwt) => {
            info!(
                "SecureStorage: loaded JWT key={} backend={}",
                JWT_KEY,
                storage_backend_label()
            );
            Ok(Some(jwt))
        }
        Err(keyring::Error::NoEntry) => {
            warn!(
                "SecureStorage: JWT not found key={} backend={}",
                JWT_KEY,
                storage_backend_label()
            );
            Ok(None)
        }
        Err(e) => Err(AuthError::StorageError(format!(
            "Failed to load JWT: {}",
            e
        ))),
    }
}

pub fn delete_jwt() -> Result<(), AuthError> {
    let entry = get_entry(JWT_KEY)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::StorageError(format!(
            "Failed to delete JWT: {}",
            e
        ))),
    }
}

pub fn store_license_key(key: &str) -> Result<(), AuthError> {
    let entry = get_entry(LICENSE_KEY)?;
    entry
        .set_password(key)
        .map_err(|e| AuthError::StorageError(format!("Failed to store license key: {}", e)))?;
    info!(
        "SecureStorage: stored license key id={} backend={}",
        LICENSE_KEY,
        storage_backend_label()
    );
    Ok(())
}

pub fn load_license_key() -> Result<Option<String>, AuthError> {
    let entry = get_entry(LICENSE_KEY)?;
    match entry.get_password() {
        Ok(key) => {
            info!(
                "SecureStorage: loaded license key id={} backend={}",
                LICENSE_KEY,
                storage_backend_label()
            );
            Ok(Some(key))
        }
        Err(keyring::Error::NoEntry) => {
            warn!(
                "SecureStorage: license key not found id={} backend={}",
                LICENSE_KEY,
                storage_backend_label()
            );
            Ok(None)
        }
        Err(e) => Err(AuthError::StorageError(format!(
            "Failed to load license key: {}",
            e
        ))),
    }
}

pub fn delete_license_key() -> Result<(), AuthError> {
    let entry = get_entry(LICENSE_KEY)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::StorageError(format!(
            "Failed to delete license key: {}",
            e
        ))),
    }
}

pub fn clear_all_credentials() -> Result<(), AuthError> {
    delete_jwt()?;
    delete_license_key()?;
    Ok(())
}
