use chrono::Utc;
use log::{info, warn};

use crate::auth::machine::machine_id::get_machine_id;
use crate::auth::state::auth_metadata::JwtMetadata;
use crate::auth::state::auth_state::AuthStatus;
use crate::auth::storage::secure_storage::load_jwt;
use crate::auth::validators::jwt_validator::{grace_period_secs, validate_jwt};

pub struct LaunchValidationResult {
    pub status: AuthStatus,
    pub jwt_metadata: Option<JwtMetadata>,
}

pub async fn run_launch_validation() -> LaunchValidationResult {
    info!("LaunchValidation: starting");

    let jwt_opt = match load_jwt() {
        Ok(j) => j,
        Err(e) => {
            warn!("LaunchValidation: failed to load JWT from keychain: {}", e);
            return LaunchValidationResult {
                status: AuthStatus::NotActivated,
                jwt_metadata: None,
            };
        }
    };

    let jwt = match jwt_opt {
        Some(j) => j,
        None => {
            info!("LaunchValidation: no JWT found - NotActivated");
            return LaunchValidationResult {
                status: AuthStatus::NotActivated,
                jwt_metadata: None,
            };
        }
    };

    let metadata = match validate_jwt(&jwt) {
        Ok(m) => m,
        Err(e) => {
            warn!("LaunchValidation: JWT validation failed: {} - Corrupted", e);
            return LaunchValidationResult {
                status: AuthStatus::Corrupted,
                jwt_metadata: None,
            };
        }
    };

    let current_machine_id = match get_machine_id() {
        Ok(id) => id,
        Err(e) => {
            warn!("LaunchValidation: could not get machine ID: {}", e);
            return LaunchValidationResult {
                status: AuthStatus::OfflineValid,
                jwt_metadata: Some(metadata),
            };
        }
    };

    if metadata.mid != current_machine_id {
        warn!(
            "LaunchValidation: machine mismatch - token_mid={} current_mid={}",
            metadata.mid, current_machine_id
        );
        return LaunchValidationResult {
            status: AuthStatus::MachineMismatch,
            jwt_metadata: Some(metadata),
        };
    }

    let now = Utc::now().timestamp();
    if metadata.expires_at < now {
        let grace_deadline = metadata.expires_at + grace_period_secs();
        if now < grace_deadline {
            warn!("LaunchValidation: JWT expired but within grace period");
            return LaunchValidationResult {
                status: AuthStatus::GracePeriod,
                jwt_metadata: Some(metadata),
            };
        }
        warn!("LaunchValidation: JWT expired and grace period passed");
        return LaunchValidationResult {
            status: AuthStatus::Expired,
            jwt_metadata: Some(metadata),
        };
    }

    let refresh_threshold_secs = 7 * 24 * 3600_i64;
    if (metadata.expires_at - now) < refresh_threshold_secs {
        info!("LaunchValidation: JWT valid but refresh recommended soon");
        return LaunchValidationResult {
            status: AuthStatus::RefreshRequired,
            jwt_metadata: Some(metadata),
        };
    }

    info!("LaunchValidation: JWT valid, tier={}", metadata.tier.as_str());
    LaunchValidationResult {
        status: AuthStatus::Valid,
        jwt_metadata: Some(metadata),
    }
}

