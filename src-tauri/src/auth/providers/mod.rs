pub mod r#trait;

#[cfg(feature = "dev-auth")]
pub mod dev_provider;
#[cfg(feature = "dev-auth")]
pub use dev_provider::DevLicenseProvider as ActiveLicenseProvider;

#[cfg(not(feature = "dev-auth"))]
pub mod production_provider;
#[cfg(not(feature = "dev-auth"))]
pub use production_provider::ProductionLicenseProvider as ActiveLicenseProvider;

#[cfg(not(feature = "dev-auth"))]
pub use production_provider::ProductionLicenseProvider as EdgeLicenseProvider;
