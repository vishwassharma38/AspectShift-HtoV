use serde::{Deserialize, Serialize};
use specta::Type;

// Activation

/// POST /api/activate - request body sent from the desktop app to the license server.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivateRequest {
    /// Gumroad license key, trimmed, validated before sending.
    pub license_key: String,
    /// Machine fingerprint from `get_machine_id()`.
    pub machine_id: String,
    /// App version string, e.g. "0.1.1".
    pub app_version: String,
    /// Build channel: "stable" | "beta" | "nightly" | "oss".
    pub channel: BuildChannel,
}

/// POST /api/activate - success response body returned from the license server.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivateResponse {
    pub ok: bool,
    pub token: String,
    pub expires_at: String,
    pub grace_period_ends_at: String,
}

/// POST /api/activate - error response body returned from the license server.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivateErrorResponse {
    pub ok: bool,
    pub error: String,
    pub message: String,
}

pub type ActivationRequest = ActivateRequest;
pub type ActivationResponse = ActivateResponse;

// Refresh

/// POST /api/refresh - request body sent from the desktop app to the license server.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefreshRequest {
    /// The current JWT.
    pub token: String,
    /// Machine fingerprint - must match the one stored at activation.
    pub machine_id: String,
}

/// POST /api/refresh - success response body (same shape as ActivateResponse, intentionally).
pub type RefreshResponse = ActivateResponse;

/// POST /api/refresh - error response body returned from the license server.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefreshErrorResponse {
    pub error: String,
}

// Update entitlement check

/// POST /updates/check - request body
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckRequest {
    /// The current JWT - edge validates this before checking update entitlement.
    pub token: String,
    /// Currently installed version.
    pub current_version: String,
}

/// POST /updates/check - response body
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase", untagged)]
pub enum UpdateCheckResponse {
    #[serde(rename_all = "camelCase")]
    Allowed {
        allowed: bool,
        latest_version: String,
        manifest_url: String,
        rollback_version: String,
    },
    NotAllowed { allowed: bool },
}

/// POST /updates/check - error response body returned from the license server.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdateCheckErrorCode {
    InvalidRequest,
    InvalidToken,
    LicenseRevoked,
    LicenseRefunded,
    ActivationRevoked,
    UpdatesNotEntitled,
    ChannelNotAllowed,
    ServerError,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckErrorResponse {
    pub ok: bool,
    pub error: UpdateCheckErrorCode,
}

// Shared value types

/// Mirrors LicenseTier in `auth/state/license_tier.rs` but uses the wire name.
/// These must stay in sync.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTierWire {
    Community,
    Pro,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum BuildChannel {
    Stable,
    Beta,
    Nightly,
    Oss,
}

/// Update entitlement block embedded in activation/refresh responses.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEntitlement {
    /// ISO-8601 timestamp: this license receives updates until this date.
    pub entitled_until: String,
    /// Whether this is a perpetual (lifetime) license with no expiry.
    pub is_perpetual: bool,
    /// Whether this license is grandfathered for versions below a cutoff.
    pub grandfathered_below_version: Option<String>,
}

// JWT claims

/// Exact JSON shape embedded in the JWT payload.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductionJwtClaims {
    /// Subject: internal license ID from Supabase (not the raw license key).
    pub sub: String,
    /// License tier: "community" | "pro".
    pub tier: String,
    /// Machine fingerprint hash: output of `get_machine_id()`.
    pub mid: String,
    /// Build channel: "stable" | "beta" | "nightly" | "oss".
    pub channel: String,
    /// Feature flags as a compact bitmask (reserved for future use, send 0 for now).
    pub flags: u32,
    /// Issued-at: Unix timestamp seconds.
    pub iat: i64,
    /// Expires-at: Unix timestamp seconds.
    pub exp: i64,
    /// Grace-expires-at: Unix timestamp seconds (exp + grace_window_secs).
    pub gexp: i64,
    /// Update entitlement expiry: Unix timestamp seconds; when no separate update entitlement
    /// exists, this falls back to the JWT `exp` claim in the runtime implementation.
    pub uexp: i64,
}

// Compile-time guard: LicenseTierWire variants must stay in sync with LicenseTier.
impl From<crate::auth::state::license_tier::LicenseTier> for LicenseTierWire {
    fn from(t: crate::auth::state::license_tier::LicenseTier) -> Self {
        match t {
            crate::auth::state::license_tier::LicenseTier::Community => Self::Community,
            crate::auth::state::license_tier::LicenseTier::Pro => Self::Pro,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateCheckResponse;

    #[test]
    fn parses_allowed_update_check_response_with_camel_case_fields() {
        let body = r#"{
            "allowed": true,
            "latestVersion": "0.2.0",
            "manifestUrl": "https://example.com/manifest.json",
            "rollbackVersion": "0.1.1"
        }"#;

        let parsed: UpdateCheckResponse =
            serde_json::from_str(body).expect("update check response should deserialize");

        match parsed {
            UpdateCheckResponse::Allowed {
                allowed,
                latest_version,
                manifest_url,
                rollback_version,
            } => {
                assert!(allowed);
                assert_eq!(latest_version, "0.2.0");
                assert_eq!(manifest_url, "https://example.com/manifest.json");
                assert_eq!(rollback_version, "0.1.1");
            }
            UpdateCheckResponse::NotAllowed { allowed } => {
                panic!("expected allowed update response, got NotAllowed({allowed})");
            }
        }
    }
}
