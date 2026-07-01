use std::env;
use std::time::Duration;

use log::warn;

use crate::auth::contracts::BuildChannel;

const DEFAULT_PRODUCTION_BASE_URL: &str = "https://aspectshift-htov-license-server.vercel.app";
const DEFAULT_DEVELOPMENT_BASE_URL: &str = "https://aspectshift-htov-license-server.vercel.app";
// Keep this aligned with the startup launch-validation timeout in AuthManager.
const DEFAULT_TIMEOUT_SECS: u64 = 12;

#[derive(Debug, Clone)]
pub struct AuthApiConfig {
    pub base_url: String,
    pub activate_url: String,
    pub refresh_url: String,
    pub updates_url: String,
    pub request_timeout_secs: u64,
}

impl AuthApiConfig {
    pub fn from_env() -> Self {
        let base_url = resolve_base_url();
        let request_timeout_secs = resolve_timeout_secs();
        let trimmed_base_url = base_url.trim_end_matches('/').to_string();

        Self {
            activate_url: format!("{}/api/activate", trimmed_base_url),
            refresh_url: format!("{}/api/refresh", trimmed_base_url),
            updates_url: format!("{}/api/updates/check", trimmed_base_url),
            base_url: trimmed_base_url,
            request_timeout_secs,
        }
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs.max(1))
    }
}

pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn current_build_channel() -> BuildChannel {
    if let Ok(channel) = env::var("ASPECTSHIFT_BUILD_CHANNEL") {
        match parse_build_channel(&channel) {
            Some(parsed) => return parsed,
            None => {
                warn!("AuthConfig: invalid ASPECTSHIFT_BUILD_CHANNEL value, defaulting to stable");
            }
        }
    }

    BuildChannel::Stable
}

fn resolve_base_url() -> String {
    env::var("ASPECTSHIFT_AUTH_BASE_URL")
        .or_else(|_| env::var("AUTH_BASE_URL"))
        .unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                DEFAULT_DEVELOPMENT_BASE_URL.to_string()
            } else {
                DEFAULT_PRODUCTION_BASE_URL.to_string()
            }
        })
}

fn resolve_timeout_secs() -> u64 {
    env::var("ASPECTSHIFT_AUTH_REQUEST_TIMEOUT_SECS")
        .or_else(|_| env::var("AUTH_REQUEST_TIMEOUT_SECS"))
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

fn parse_build_channel(value: &str) -> Option<BuildChannel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stable" => Some(BuildChannel::Stable),
        "beta" => Some(BuildChannel::Beta),
        "nightly" => Some(BuildChannel::Nightly),
        "oss" => Some(BuildChannel::Oss),
        _ => None,
    }
}
