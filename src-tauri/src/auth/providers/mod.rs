pub mod r#trait;

#[cfg(feature = "dev-auth")]
pub mod dev_provider;
#[cfg(feature = "dev-auth")]
pub use dev_provider::DevLicenseProvider as ActiveLicenseProvider;

#[cfg(not(feature = "dev-auth"))]
pub mod edge_provider;
#[cfg(not(feature = "dev-auth"))]
pub use edge_provider::EdgeLicenseProvider as ActiveLicenseProvider;
