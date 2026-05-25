use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTier {
    Community,
    Licensed,
}

impl Default for LicenseTier {
    fn default() -> Self {
        Self::Community
    }
}

impl LicenseTier {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "licensed" => Self::Licensed,
            _ => Self::Community,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Community => "community",
            Self::Licensed => "licensed",
        }
    }

    pub fn is_licensed(&self) -> bool {
        matches!(self, Self::Licensed)
    }
}


