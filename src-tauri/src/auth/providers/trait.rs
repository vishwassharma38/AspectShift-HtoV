use std::future::Future;
use std::pin::Pin;

use crate::auth::auth_models::UpdateEntitlementCheckResult;
use crate::auth::auth_errors::AuthError;
use crate::auth::state::auth_state::AuthStatus;

pub type LicenseToken = String;
pub type ActivationResponse = LicenseToken;
pub type RefreshResponse = LicenseToken;
pub type EntitlementClaims = AuthStatus;

pub trait LicenseProvider: Send + Sync {
    fn activate<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<ActivationResponse, AuthError>> + Send + 'a>>;
    fn refresh<'a>(
        &'a self,
        token: &'a LicenseToken,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshResponse, AuthError>> + Send + 'a>>;
    fn check_updates<'a>(
        &'a self,
        token: &'a LicenseToken,
        current_version: &'a str,
    ) -> Pin<
        Box<dyn Future<Output = Result<UpdateEntitlementCheckResult, AuthError>> + Send + 'a>,
    >;
    fn validate<'a>(
        &'a self,
        token: &'a LicenseToken,
    ) -> Pin<Box<dyn Future<Output = Result<EntitlementClaims, AuthError>> + Send + 'a>>;
    fn deactivate<'a>(
        &'a self,
        token: &'a LicenseToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuthError>> + Send + 'a>>;
}
