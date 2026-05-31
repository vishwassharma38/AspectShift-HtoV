pub mod activation;
pub mod auth_commands;
pub mod auth_errors;
pub mod auth_events;
pub mod auth_models;
pub mod contracts;
pub mod outcome_mapping;
pub mod refresh;

pub mod manager {
    pub mod auth_manager;
}

pub mod state {
    pub mod auth_metadata;
    pub mod auth_state;
    pub mod license_tier;
}

pub mod providers;

pub mod validators {
    pub mod entitlement_validator;
    pub mod jwt_validator;
    pub mod launch_validation;
}

pub mod storage {
    pub mod secure_storage;
}

pub mod crypto {
    pub mod jwt;
}

pub mod machine {
    pub mod machine_id;
}
