use chrono::Utc;
use log::{info, warn};

use crate::auth::machine::machine_id::get_machine_id;
use crate::auth::outcome_mapping::map_auth_error;
use crate::auth::state::auth_metadata::JwtMetadata;
use crate::auth::state::auth_state::AuthStatus;
use crate::auth::storage::secure_storage::load_jwt;
use crate::auth::validators::jwt_validator::{classify_launch_status, validate_jwt};

pub struct LaunchValidationResult {
    pub status: AuthStatus,
    pub jwt_metadata: Option<JwtMetadata>,
}

pub async fn run_launch_validation() -> LaunchValidationResult {
    info!("LaunchValidation: starting");

    let jwt_opt_res = tokio::task::spawn_blocking(|| load_jwt()).await;
    let jwt_opt = match jwt_opt_res {
        Ok(Ok(j)) => j,
        Ok(Err(e)) => {
            warn!("LaunchValidation: failed to load JWT from keychain: {}", e);
            return LaunchValidationResult {
                status: AuthStatus::NotActivated,
                jwt_metadata: None,
            };
        }
        Err(e) => {
            warn!("LaunchValidation: keychain access task panicked: {}", e);
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
            let status = map_auth_error(&e).status;
            warn!(
                "LaunchValidation: JWT validation failed: {} - {:?}",
                e, status
            );
            return LaunchValidationResult {
                status,
                jwt_metadata: None,
            };
        }
    };

    let mid_res = tokio::task::spawn_blocking(|| get_machine_id()).await;
    let current_machine_id = match mid_res {
        Ok(Ok(id)) => id,
        Ok(Err(e)) => {
            warn!("LaunchValidation: could not get machine ID: {}", e);
            return LaunchValidationResult {
                status: AuthStatus::OfflineValid,
                jwt_metadata: Some(metadata),
            };
        }
        Err(e) => {
            warn!("LaunchValidation: machine ID task panicked: {}", e);
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
    let launch_status = classify_launch_status(&metadata, now);

    match launch_status {
        AuthStatus::GracePeriod => {
            warn!("LaunchValidation: JWT expired but within grace period");
        }
        AuthStatus::Expired => {
            warn!("LaunchValidation: JWT expired and grace period passed");
        }
        AuthStatus::RefreshRequired => {
            info!("LaunchValidation: JWT valid but refresh recommended soon");
        }
        AuthStatus::Valid => {
            info!(
                "LaunchValidation: JWT valid, tier={}",
                metadata.tier.as_str()
            );
        }
        _ => {}
    }

    LaunchValidationResult {
        status: launch_status,
        jwt_metadata: Some(metadata),
    }
}
