use chrono::Utc;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};

use crate::auth::auth_errors::AuthError;
use crate::auth::state::auth_metadata::{JwtClaims, JwtMetadata};
use crate::auth::validators::jwt_validator::{build_jwt_metadata, validate_jwt_timing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyStatus {
    Active,
    Retired,
}

#[derive(Clone, Copy)]
struct ProductionKey {
    kid: &'static str,
    public_key_pem: &'static [u8],
    status: KeyStatus,
}

impl ProductionKey {
    fn is_active(self) -> bool {
        matches!(self.status, KeyStatus::Active)
    }
}

/// Production public keyring.
/// Embedded directly in the binary for offline verification.
const PROD_KEYS: &[ProductionKey] = &[
    ProductionKey {
        kid: "2026-q2",
        public_key_pem: include_bytes!("keys/2026-q2.pub"),
        status: KeyStatus::Active,
    },
    ProductionKey {
        kid: "2026-q4",
        public_key_pem: include_bytes!("keys/2026-q4.pub"),
        status: KeyStatus::Retired,
    },
];

fn validate_kid_format(kid: &str) -> Result<(), AuthError> {
    let trimmed = kid.trim();
    if trimmed.is_empty() || trimmed != kid {
        return Err(AuthError::JwtError(
            "Malformed key ID (kid) in JWT header".to_string(),
        ));
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AuthError::JwtError(
            "Malformed key ID (kid) in JWT header".to_string(),
        ));
    }

    Ok(())
}

fn resolve_key(kid: &str) -> Result<&'static ProductionKey, AuthError> {
    validate_kid_format(kid)?;

    let key = PROD_KEYS
        .iter()
        .find(|entry| entry.kid == kid)
        .ok_or_else(|| AuthError::JwtError(format!("Unknown key ID (kid): {}", kid)))?;

    if !key.is_active() {
        return Err(AuthError::JwtError(format!(
            "Retired key ID (kid): {}",
            kid
        )));
    }

    Ok(key)
}

/// Validates a JWT using the embedded production keyring and EdDSA (Ed25519).
/// This is used in production builds.
pub fn validate_production_jwt(token: &str) -> Result<JwtMetadata, AuthError> {
    let header = decode_header(token).map_err(|e| AuthError::JwtError(e.to_string()))?;

    if header.alg != Algorithm::EdDSA {
        return Err(AuthError::JwtError(
            "Unsupported algorithm: expected EdDSA".to_string(),
        ));
    }

    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| AuthError::JwtError("Missing 'kid' in JWT header".to_string()))?;

    let key = resolve_key(kid)?;
    let decoding_key = DecodingKey::from_ed_pem(key.public_key_pem)
        .map_err(|e| AuthError::JwtError(format!("Failed to parse public key: {}", e)))?;

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = false;
    validation.required_spec_claims.remove("iss");
    validation.required_spec_claims.remove("aud");

    let token_data = decode::<JwtClaims>(token, &decoding_key, &validation)
        .map_err(|e| AuthError::JwtError(format!("JWT validation failed: {}", e)))?;

    let metadata = build_jwt_metadata(token_data.claims)?;
    validate_jwt_timing(&metadata, Utc::now().timestamp())?;

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::state::license_tier::LicenseTier;
    use jsonwebtoken::decode_header;

    fn base64url_encode(data: &[u8]) -> String {
        let standard = base64_encode(data);
        standard
            .replace('+', "-")
            .replace('/', "_")
            .replace('=', "")
    }

    fn base64_encode(data: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();
        let mut i = 0;
        while i < data.len() {
            let b0 = data[i] as usize;
            let b1 = if i + 1 < data.len() {
                data[i + 1] as usize
            } else {
                0
            };
            let b2 = if i + 2 < data.len() {
                data[i + 2] as usize
            } else {
                0
            };
            result.push(CHARS[b0 >> 2] as char);
            result.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
            if i + 1 < data.len() {
                result.push(CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char);
            } else {
                result.push('=');
            }
            if i + 2 < data.len() {
                result.push(CHARS[b2 & 0x3f] as char);
            } else {
                result.push('=');
            }
            i += 3;
        }
        result
    }

    fn jwt_token(header: serde_json::Value, payload: serde_json::Value) -> String {
        let header = base64url_encode(header.to_string().as_bytes());
        let payload = base64url_encode(payload.to_string().as_bytes());
        format!("{}.{}.sig", header, payload)
    }

    #[test]
    fn test_production_jwt_verification() {
        let token = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtcTIifQ.eyJzdWIiOiIwMzkwNzI0My05M2RjLTRmNTUtODBlZC1jMjhjMGQ3YzQ3YTEiLCJ0aWVyIjoicHJvIiwibWlkIjoibWFjaGluZS1hYmMtMTIzIiwiY2hhbm5lbCI6InN0YWJsZSIsImZsYWdzIjowLCJpYXQiOjE3Nzk4NjIyNTYsImV4cCI6MTc4MDQ2NzA1NiwiZ2V4cCI6MTc4MDYzOTg1NiwidWV4cCI6MTgxMTMzNDEyM30.cSzQ2OKoElVV3kYf89CjxWfFFM35Dt-wTklwXWEvb-_3NrN23H7YX0BmLM2X9V9mNhXCHzA445QQAj6OGoPPCA";

        let result = validate_production_jwt(token);

        match result {
            Ok(metadata) => {
                assert_eq!(metadata.sub, "03907243-93dc-4f55-80ed-c28c0d7c47a1");
                assert_eq!(metadata.mid, "machine-abc-123");
                assert_eq!(metadata.tier, LicenseTier::Pro);
            }
            Err(AuthError::LicenseExpired) => {
                // The signature and claims were still accepted, but the token aged out.
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_multi_key_rotation_lookup() {
        let q2 = resolve_key("2026-q2").expect("q2 key should be active");
        assert!(q2.is_active());
        assert_eq!(q2.kid, "2026-q2");

        let q4 = resolve_key("2026-q4");
        assert!(matches!(q4, Err(AuthError::JwtError(e)) if e.contains("Retired key ID")));
    }

    #[test]
    fn test_mismatched_key_fails() {
        let token = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtcTQifQ.eyJzdWIiOiJ1c2VyXzEyMyIsInRpZXIiOiJwcm8iLCJtaWQiOiJtYWNoaW5lX2FiYyIsImNoYW5uZWwiOiJzdGFibGUiLCJmbGFncyI6MSwiaWF0IjoxNzc5NzYzNjc4LCJleHAiOjE3Nzk3NjcyNzgsImdleHAiOjE3Nzk3NzA4NzgsInVleHAiOjE3Nzk3NjcyNzh9.Vb4U2X0EHF_vS45MDJT3V4wdeWM_GEHQx0M2QmXLDCt7vlKeczB_O4o8kEMK9hu5ycLbS3AJiS7qRCGNsV2aBg";
        let result = validate_production_jwt(token);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_tier_fails() {
        let claims = JwtClaims {
            sub: "user_123".to_string(),
            tier: "enterprise".to_string(),
            mid: "machine_abc".to_string(),
            channel: "stable".to_string(),
            flags: 1,
            iat: 1779763678,
            exp: 1779767278,
            gexp: 1779770878,
            uexp: 1779767278,
            unknown_fields: std::collections::HashMap::new(),
        };

        let result = build_jwt_metadata(claims);
        assert!(matches!(result, Err(AuthError::InvalidLicenseTier)));
    }

    #[test]
    fn test_tamper_token() {
        let token = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtcTIifQ.eyJzdWIiOiJ1c2VyXzEyMyIsInRpZXIiOiJwcm8iLCJtaWQiOiJtYWNoaW5lX2FiYyIsImNoYW5uZWwiOiJzdGFibGUiLCJmbGFncyI6MSwiaWF0IjoxNzc5NzYzNjc4LCJleHAiOjE3Nzk3NjcyNzgsImdleHAiOjE3Nzk3NzA4NzgsInVleHAiOjE3Nzk3NjcyNzh9.Vb4U2X0EHF_vS45MDJT3V4wdeWM_GEHQx0M2QmXLDCt7vlKeczB_O4o8kEMK9hu5ycLbS3AJiS7qRCGNsV2aBg";

        let mut tampered_token = token.to_string();
        tampered_token.replace_range(60..61, "A");

        let result = validate_production_jwt(&tampered_token);
        assert!(result.is_err(), "Tampered token should fail verification");
    }

    #[test]
    fn test_unknown_kid() {
        let token = jwt_token(
            serde_json::json!({"alg":"EdDSA","kid":"unknown"}),
            serde_json::json!({"sub":"user_123"}),
        );
        let result = validate_production_jwt(&token);
        assert!(matches!(result, Err(AuthError::JwtError(e)) if e.contains("Unknown key ID")));
    }

    #[test]
    fn test_missing_kid() {
        let token = jwt_token(
            serde_json::json!({"alg":"EdDSA"}),
            serde_json::json!({"sub":"user_123"}),
        );
        let result = validate_production_jwt(&token);
        assert!(matches!(result, Err(AuthError::JwtError(e)) if e.contains("Missing 'kid'")));
    }

    #[test]
    fn test_malformed_kid() {
        let token = jwt_token(
            serde_json::json!({"alg":"EdDSA","kid":"bad kid!"}),
            serde_json::json!({"sub":"user_123"}),
        );
        let result = validate_production_jwt(&token);
        assert!(matches!(result, Err(AuthError::JwtError(e)) if e.contains("Malformed key ID")));
    }

    #[test]
    fn test_alg_none_fails() {
        let token = jwt_token(
            serde_json::json!({"alg":"none","kid":"2026-q2"}),
            serde_json::json!({"sub":"user_123"}),
        );
        let result = validate_production_jwt(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_header_parses_before_key_lookup() {
        let header = decode_header("eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtcTIifQ.payload.sig")
            .expect("header should parse");
        assert_eq!(header.alg, Algorithm::EdDSA);
    }
}
