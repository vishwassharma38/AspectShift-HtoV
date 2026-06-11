use sha2::{Digest, Sha256};

use crate::auth::auth_errors::AuthError;

const MACHINE_ID_PREFIX: &str = "mid_";
const MACHINE_ID_LENGTH: usize = 8;

pub fn get_machine_id() -> Result<String, AuthError> {
    let raw = get_raw_machine_id()?;
    let hashed = hash_machine_id(&raw);
    Ok(format_machine_id(&hashed))
}

fn hash_machine_id(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..])
}

fn format_machine_id(hashed: &str) -> String {
    let truncated = &hashed[..MACHINE_ID_LENGTH.min(hashed.len())];
    format!("{}{}", MACHINE_ID_PREFIX, truncated.to_uppercase())
}

#[cfg(target_os = "windows")]
fn get_raw_machine_id() -> Result<String, AuthError> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut command = Command::new("reg");
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .args([
            "query",
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .map_err(|e| AuthError::MachineIdError(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.contains("MachineGuid") {
            if let Some(guid) = trimmed.split_whitespace().last() {
                return Ok(guid.to_string());
            }
        }
    }
    get_hostname_fallback()
}

#[cfg(target_os = "macos")]
fn get_raw_machine_id() -> Result<String, AuthError> {
    use std::process::Command;

    let output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| AuthError::MachineIdError(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            if let Some(start) = line.find('"') {
                let rest = &line[start + 1..];
                if let Some(end) = rest.rfind('"') {
                    return Ok(rest[..end].to_string());
                }
            }
        }
    }
    get_hostname_fallback()
}

#[cfg(target_os = "linux")]
fn get_raw_machine_id() -> Result<String, AuthError> {
    match std::fs::read_to_string("/etc/machine-id") {
        Ok(id) => Ok(id.trim().to_string()),
        Err(_) => match std::fs::read_to_string("/var/lib/dbus/machine-id") {
            Ok(id) => Ok(id.trim().to_string()),
            Err(_) => get_hostname_fallback(),
        },
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn get_raw_machine_id() -> Result<String, AuthError> {
    get_hostname_fallback()
}

fn get_hostname_fallback() -> Result<String, AuthError> {
    std::fs::read_to_string("/etc/hostname")
        .map(|h| h.trim().to_string())
        .or_else(|_| {
            std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .map_err(|e| {
                    AuthError::MachineIdError(format!("All machine ID sources failed: {}", e))
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_format_is_correct() {
        let id = get_machine_id();
        assert!(id.is_ok(), "machine_id should not fail: {:?}", id);
        let id = id.unwrap();
        assert!(
            id.starts_with("mid_"),
            "should start with mid_ prefix: {}",
            id
        );
        assert_eq!(id.len(), MACHINE_ID_PREFIX.len() + MACHINE_ID_LENGTH);
    }

    #[test]
    fn machine_id_is_stable() {
        let id1 = get_machine_id().unwrap();
        let id2 = get_machine_id().unwrap();
        assert_eq!(id1, id2, "machine_id must be deterministic");
    }
}
