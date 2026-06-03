use std::sync::Arc;

use chrono::DateTime;
use log::{info, warn};
use reqwest::StatusCode;

use crate::auth::auth_errors::AuthError;
use crate::auth::config::auth_config::{app_version, current_build_channel, AuthApiConfig};
use crate::auth::contracts::{
    ActivateErrorResponse, ActivateRequest, ActivateResponse, RefreshErrorResponse, RefreshRequest,
    RefreshResponse as RefreshApiResponse,
};
use crate::auth::machine::machine_id::get_machine_id;
use crate::auth::providers::r#trait::{
    ActivationResponse, EntitlementClaims, LicenseProvider, LicenseToken, RefreshResponse,
};
use crate::auth::validators::entitlement_validator::validate_license_key_format;

pub struct ProductionLicenseProvider {
    client: reqwest::Client,
    config: Arc<AuthApiConfig>,
}

impl ProductionLicenseProvider {
    pub fn new() -> Self {
        let config = Arc::new(AuthApiConfig::from_env());
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout())
            .build()
            .expect("reqwest client must be constructible");

        Self { client, config }
    }

    async fn activate_remote(&self, license_key: &str) -> Result<String, AuthError> {
        validate_license_key_format(license_key)?;

        let machine_id = get_machine_id()?;
        let request = ActivateRequest {
            license_key: license_key.trim().to_string(),
            machine_id,
            app_version: app_version().to_string(),
            channel: current_build_channel(),
        };

        info!(
            "ProductionAuthProvider: starting activation against {}",
            self.config.activate_url
        );

        let response = self
            .client
            .post(&self.config.activate_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                warn!("ProductionAuthProvider: activation network error: {}", e);
                AuthError::ActivationFailed {
                    reason: format!("Network error: {}", e),
                }
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            warn!(
                "ProductionAuthProvider: activation response read failed: {}",
                e
            );
            AuthError::ActivationFailed {
                reason: format!("Failed to read activation response: {}", e),
            }
        })?;

        if status.is_success() {
            let payload = parse_activate_response(&body)?;
            if !payload.ok {
                return Err(AuthError::ActivationFailed {
                    reason: "License server returned ok=false for a success response".to_string(),
                });
            }

            if payload.token.trim().is_empty() {
                return Err(AuthError::ActivationFailed {
                    reason: "Activation response missing token".to_string(),
                });
            }

            DateTime::parse_from_rfc3339(&payload.expires_at).map_err(|e| {
                AuthError::ActivationFailed {
                    reason: format!("Activation response had an invalid expiresAt value: {}", e),
                }
            })?;

            info!("ProductionAuthProvider: activation succeeded");
            return Ok(payload.token);
        }

        match parse_activate_error(&body) {
            Ok(err) if !err.ok => Err(map_activate_error(status, err)),
            Ok(_) => Err(AuthError::ActivationFailed {
                reason: "License server returned ok=true in an error response".to_string(),
            }),
            Err(parse_err) => {
                warn!(
                    "ProductionAuthProvider: activation failed with HTTP {} and unparseable body",
                    status
                );
                Err(AuthError::ActivationFailed {
                    reason: format!("HTTP {} from activation endpoint: {}", status, parse_err),
                })
            }
        }
    }

    async fn refresh_remote(&self, token: &str) -> Result<String, AuthError> {
        if token.trim().is_empty() {
            return Err(AuthError::RefreshFailed {
                reason: "INVALID_TOKEN: refresh token must not be empty".to_string(),
            });
        }

        let machine_id = get_machine_id()?;
        let request = RefreshRequest {
            token: token.trim().to_string(),
            machine_id,
        };

        info!(
            "ProductionAuthProvider: starting refresh against {}",
            self.config.refresh_url
        );

        let response = self
            .client
            .post(&self.config.refresh_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                warn!("ProductionAuthProvider: refresh network error: {}", e);
                AuthError::RefreshFailed {
                    reason: format!("Network error: {}", e),
                }
            })?;

        info!("ProductionAuthProvider: refresh request sent");

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            warn!(
                "ProductionAuthProvider: refresh response read failed: {}",
                e
            );
            AuthError::RefreshFailed {
                reason: format!("Failed to read refresh response: {}", e),
            }
        })?;

        if status.is_success() {
            let payload = parse_refresh_response(&body)?;
            if !payload.ok {
                return Err(AuthError::RefreshFailed {
                    reason: "License server returned ok=false for a success response".to_string(),
                });
            }

            if payload.token.trim().is_empty() {
                return Err(AuthError::RefreshFailed {
                    reason: "Refresh response missing token".to_string(),
                });
            }

            DateTime::parse_from_rfc3339(&payload.expires_at).map_err(|e| {
                AuthError::RefreshFailed {
                    reason: format!("Refresh response had an invalid expiresAt value: {}", e),
                }
            })?;

            info!("ProductionAuthProvider: refresh succeeded");
            return Ok(payload.token);
        }

        match parse_refresh_error(&body) {
            Ok(err) => Err(map_refresh_error(status, err)),
            Err(parse_err) => {
                warn!(
                    "ProductionAuthProvider: refresh failed with HTTP {} and unparseable body",
                    status
                );
                Err(AuthError::RefreshFailed {
                    reason: format!("HTTP {} from refresh endpoint: {}", status, parse_err),
                })
            }
        }
    }
}

impl Default for ProductionLicenseProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LicenseProvider for ProductionLicenseProvider {
    fn activate<'a>(
        &'a self,
        key: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ActivationResponse, AuthError>> + Send + 'a>,
    > {
        Box::pin(async move { self.activate_remote(key).await })
    }

    fn refresh<'a>(
        &'a self,
        token: &'a LicenseToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<RefreshResponse, AuthError>> + Send + 'a>,
    > {
        Box::pin(async move { self.refresh_remote(token).await })
    }

    fn validate<'a>(
        &'a self,
        _token: &'a LicenseToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<EntitlementClaims, AuthError>> + Send + 'a>,
    > {
        Box::pin(async { Err(AuthError::PhaseDNotImplemented) })
    }

    fn deactivate<'a>(
        &'a self,
        _token: &'a LicenseToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AuthError>> + Send + 'a>>
    {
        Box::pin(async { Err(AuthError::PhaseDNotImplemented) })
    }
}

fn parse_activate_response(body: &str) -> Result<ActivateResponse, AuthError> {
    serde_json::from_str::<ActivateResponse>(body).map_err(|e| AuthError::ActivationFailed {
        reason: format!("Malformed activation success payload: {}", e),
    })
}

fn parse_activate_error(body: &str) -> Result<ActivateErrorResponse, AuthError> {
    serde_json::from_str::<ActivateErrorResponse>(body).map_err(|e| AuthError::ActivationFailed {
        reason: format!("Malformed activation error payload: {}", e),
    })
}

fn parse_refresh_response(body: &str) -> Result<RefreshApiResponse, AuthError> {
    serde_json::from_str::<RefreshApiResponse>(body).map_err(|e| AuthError::RefreshFailed {
        reason: format!("Malformed refresh success payload: {}", e),
    })
}

fn parse_refresh_error(body: &str) -> Result<RefreshErrorResponse, AuthError> {
    serde_json::from_str::<RefreshErrorResponse>(body).map_err(|e| AuthError::RefreshFailed {
        reason: format!("Malformed refresh error payload: {}", e),
    })
}

fn map_activate_error(status: StatusCode, payload: ActivateErrorResponse) -> AuthError {
    let error_code = payload.error.trim().to_ascii_uppercase();

    match error_code.as_str() {
        "INVALID_REQUEST" => AuthError::InvalidRequest,
        "LICENSE_NOT_FOUND" => AuthError::LicenseNotFound,
        "LICENSE_REVOKED" => AuthError::LicenseRevoked,
        "LICENSE_REFUNDED" => AuthError::LicenseRefunded,
        "ACTIVATION_LIMIT_REACHED" => AuthError::ActivationLimitReached,
        "SERVER_ERROR" => AuthError::ServerError,
        "LICENSE_EXPIRED" => AuthError::LicenseExpired,
        other => {
            warn!(
                "ProductionAuthProvider: activation failed with HTTP {} and backend code {}",
                status, other
            );
            AuthError::ActivationFailed {
                reason: format!("{}: {}", other, payload.message),
            }
        }
    }
}

fn map_refresh_error(status: StatusCode, payload: RefreshErrorResponse) -> AuthError {
    let error_code = payload.error.trim().to_ascii_uppercase();

    match error_code.as_str() {
        "INVALID_REQUEST" => AuthError::InvalidRequest,
        "INVALID_TOKEN" => AuthError::TokenCorrupted,
        "LICENSE_REVOKED" => AuthError::LicenseRevoked,
        "LICENSE_REFUNDED" => AuthError::LicenseRefunded,
        "MACHINE_MISMATCH" => AuthError::MachineMismatch,
        "ACTIVATION_REVOKED" => AuthError::LicenseRevoked,
        "SERVER_ERROR" => AuthError::ServerError,
        other => {
            warn!(
                "ProductionAuthProvider: refresh failed with HTTP {} and backend code {}",
                status, other
            );
            AuthError::RefreshFailed {
                reason: other.to_string(),
            }
        }
    }
}
