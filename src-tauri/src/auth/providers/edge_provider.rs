use crate::auth::auth_errors::AuthError;
use crate::auth::auth_models::UpdateEntitlementCheckResult;
use crate::auth::providers::r#trait::{
    ActivationResponse, EntitlementClaims, LicenseProvider, LicenseToken, RefreshResponse,
};

pub struct EdgeLicenseProvider;

impl LicenseProvider for EdgeLicenseProvider {
    fn activate<'a>(
        &'a self,
        _key: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ActivationResponse, AuthError>> + Send + 'a>,
    > {
        Box::pin(async { Err(AuthError::PhaseDNotImplemented) })
    }

    fn refresh<'a>(
        &'a self,
        _token: &'a LicenseToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<RefreshResponse, AuthError>> + Send + 'a>,
    > {
        Box::pin(async { Err(AuthError::PhaseDNotImplemented) })
    }

    fn check_updates<'a>(
        &'a self,
        _token: &'a LicenseToken,
        _current_version: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<UpdateEntitlementCheckResult, AuthError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async { Err(AuthError::PhaseDNotImplemented) })
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
