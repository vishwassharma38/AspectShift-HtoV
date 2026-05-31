use crate::auth::auth_errors::AuthError;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTier {
    Community,
    Pro,
}

impl Default for LicenseTier {
    fn default() -> Self {
        Self::Community
    }
}

impl LicenseTier {
    pub fn from_str(s: &str) -> Result<Self, AuthError> {
        match s.to_lowercase().as_str() {
            "community" => Ok(Self::Community),
            "pro" | "licensed" => Ok(Self::Pro),
            _ => Err(AuthError::InvalidLicenseTier),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Community => "community",
            Self::Pro => "pro",
        }
    }

    pub fn is_licensed(&self) -> bool {
        matches!(self, Self::Pro)
    }
}
