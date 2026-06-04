use serde::{Deserialize, Serialize};

use crate::auth::state::license_tier::LicenseTier;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: String,
    pub tier: String,
    pub mid: String,
    pub channel: String,
    pub flags: u32,
    pub iat: i64,
    pub exp: i64,
    pub gexp: i64,
    pub uexp: i64,

    /// Catch-all for future claims to ensure forward compatibility.
    /// This allows the server to add new fields without breaking older clients.
    #[serde(flatten)]
    pub unknown_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfigMetadata {
    pub machine_id: Option<String>,
    pub activated_at: Option<String>,
    pub jwt_expires_at: Option<String>,
    pub grace_expires_at: Option<String>,
    pub update_expires_at: Option<String>,
    pub last_refresh_at: Option<String>,
    pub last_validation_at: Option<String>,
    pub token_hint: Option<String>,
    pub build_channel: Option<String>,
    pub purchase_token_hint: Option<String>,

    // D7.6 Grace & Diagnostic fields
    pub grace_started_at: Option<String>,
    pub last_refresh_attempt_at: Option<String>,
    pub last_refresh_success_at: Option<String>,
    pub last_refresh_failure_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthPersistenceEnvelope {
    pub schema_version: u32,
    pub auth: AuthConfigMetadata,
}

impl AuthPersistenceEnvelope {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn new(auth: AuthConfigMetadata) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            auth,
        }
    }
}

#[derive(Debug, Clone)]
pub struct JwtMetadata {
    pub sub: String,
    pub tier: LicenseTier,
    pub mid: String,
    pub channel: String,
    pub flags: u32,
    pub issued_at: i64,
    pub expires_at: i64,
    pub grace_expires_at: i64,
    pub update_expires_at: i64,
}

impl JwtMetadata {
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now > self.grace_expires_at
    }

    pub fn is_in_grace_period(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now > self.expires_at && now <= self.grace_expires_at
    }

    pub fn expires_in_secs(&self) -> i64 {
        let now = chrono::Utc::now().timestamp();
        (self.expires_at - now).max(0)
    }
}
