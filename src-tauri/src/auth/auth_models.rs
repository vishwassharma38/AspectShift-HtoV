use serde::{Deserialize, Serialize};
use specta::Type;

use crate::auth::state::auth_state::AuthState;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivationResult {
    pub success: bool,
    pub auth_state: AuthState,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckAvailableResult {
    pub latest_version: String,
    pub manifest_url: String,
    pub rollback_version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateEntitlementCheckStatus {
    UpdateAvailable,
    NoUpdate,
    NotEntitled,
    ChannelNotAllowed,
    LicenseRevoked,
    LicenseRefunded,
    AuthRequired,
    Offline,
    ServerError,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEntitlementCheckResult {
    pub status: UpdateEntitlementCheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<UpdateCheckAvailableResult>,
}

impl UpdateEntitlementCheckResult {
    pub fn update_available(data: UpdateCheckAvailableResult) -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::UpdateAvailable,
            data: Some(data),
        }
    }

    pub fn no_update() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::NoUpdate,
            data: None,
        }
    }

    pub fn not_entitled() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::NotEntitled,
            data: None,
        }
    }

    pub fn channel_not_allowed() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::ChannelNotAllowed,
            data: None,
        }
    }

    pub fn license_revoked() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::LicenseRevoked,
            data: None,
        }
    }

    pub fn license_refunded() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::LicenseRefunded,
            data: None,
        }
    }

    pub fn auth_required() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::AuthRequired,
            data: None,
        }
    }

    pub fn offline() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::Offline,
            data: None,
        }
    }

    pub fn server_error() -> Self {
        Self {
            status: UpdateEntitlementCheckStatus::ServerError,
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UpdateEntitlementCheckResult, UpdateEntitlementCheckStatus};

    #[test]
    fn preserves_revoked_and_refunded_update_outcomes() {
        assert!(matches!(
            UpdateEntitlementCheckResult::license_revoked().status,
            UpdateEntitlementCheckStatus::LicenseRevoked
        ));
        assert!(matches!(
            UpdateEntitlementCheckResult::license_refunded().status,
            UpdateEntitlementCheckStatus::LicenseRefunded
        ));
    }
}
