use serde::{Deserialize, Serialize};
use specta::Type;

use crate::auth::state::license_tier::LicenseTier;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    NotActivated,
    Activating,
    Valid,
    GracePeriod,
    RefreshRequired,
    Invalid,
    Expired,
    OfflineValid,
    MachineMismatch,
    Corrupted,
}

impl Default for AuthStatus {
    fn default() -> Self {
        Self::NotActivated
    }
}

impl AuthStatus {
    pub fn allows_access(&self) -> bool {
        matches!(self, Self::Valid | Self::GracePeriod | Self::OfflineValid)
    }

    pub fn needs_refresh(&self) -> bool {
        matches!(self, Self::RefreshRequired | Self::GracePeriod)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub status: AuthStatus,
    pub tier: LicenseTier,
    pub activated_at: Option<String>,
    pub jwt_expires_at: Option<String>,
    pub token_hint: Option<String>,
    pub machine_id: Option<String>,
}

