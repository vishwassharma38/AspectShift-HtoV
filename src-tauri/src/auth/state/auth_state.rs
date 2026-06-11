use serde::{Deserialize, Serialize};
use specta::Type;

use crate::auth::state::license_tier::LicenseTier;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Initializing,
    CredentialsFound,
    Validating,
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
    RecoverableError,
}

impl Default for AuthStatus {
    fn default() -> Self {
        Self::Initializing
    }
}

impl AuthStatus {
    pub fn allows_access(&self) -> bool {
        matches!(
            self,
            Self::Valid | Self::RefreshRequired | Self::GracePeriod | Self::OfflineValid
        )
    }

    pub fn is_startup_pending(&self) -> bool {
        matches!(
            self,
            Self::Initializing | Self::CredentialsFound | Self::Validating
        )
    }

    pub fn needs_refresh(&self) -> bool {
        matches!(self, Self::RefreshRequired | Self::GracePeriod)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_access() {
        assert!(AuthStatus::Valid.allows_access());
        assert!(AuthStatus::RefreshRequired.allows_access());
        assert!(AuthStatus::GracePeriod.allows_access());
        assert!(AuthStatus::OfflineValid.allows_access());

        assert!(!AuthStatus::Initializing.allows_access());
        assert!(!AuthStatus::CredentialsFound.allows_access());
        assert!(!AuthStatus::Validating.allows_access());
        assert!(!AuthStatus::NotActivated.allows_access());
        assert!(!AuthStatus::Activating.allows_access());
        assert!(!AuthStatus::Invalid.allows_access());
        assert!(!AuthStatus::Expired.allows_access());
        assert!(!AuthStatus::MachineMismatch.allows_access());
        assert!(!AuthStatus::Corrupted.allows_access());
        assert!(!AuthStatus::RecoverableError.allows_access());
    }

    #[test]
    fn test_needs_refresh() {
        assert!(AuthStatus::RefreshRequired.needs_refresh());
        assert!(AuthStatus::GracePeriod.needs_refresh());

        assert!(!AuthStatus::Valid.needs_refresh());
        assert!(!AuthStatus::Expired.needs_refresh());
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub status: AuthStatus,
    pub tier: LicenseTier,
    pub activated_at: Option<String>,
    pub jwt_expires_at: Option<String>,
    pub grace_expires_at: Option<String>,
    pub token_hint: Option<String>,
    pub machine_id: Option<String>,
}
