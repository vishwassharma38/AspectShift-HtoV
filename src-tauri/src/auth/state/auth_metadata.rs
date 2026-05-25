use serde::{Deserialize, Serialize};

use crate::auth::state::license_tier::LicenseTier;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: String,
    pub tier: String,
    pub mid: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfigMetadata {
    pub machine_id: Option<String>,
    pub activated_at: Option<String>,
    pub jwt_expires_at: Option<String>,
    pub token_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JwtMetadata {
    pub sub: String,
    pub tier: LicenseTier,
    pub mid: String,
    pub issued_at: i64,
    pub expires_at: i64,
}

impl JwtMetadata {
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.expires_at < now
    }

    pub fn expires_in_secs(&self) -> i64 {
        let now = chrono::Utc::now().timestamp();
        (self.expires_at - now).max(0)
    }
}

