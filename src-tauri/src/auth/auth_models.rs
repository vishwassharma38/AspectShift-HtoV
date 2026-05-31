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
