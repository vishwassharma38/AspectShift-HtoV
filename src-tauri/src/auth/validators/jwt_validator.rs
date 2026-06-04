use crate::auth::auth_errors::AuthError;
use crate::auth::state::auth_metadata::JwtMetadata;
use crate::auth::state::auth_state::AuthStatus;
use crate::auth::state::license_tier::LicenseTier;

// grace_period_secs is unconditionally needed by launch_validation.rs
pub fn grace_period_secs() -> i64 {
    const GRACE_PERIOD_DAYS: i64 = 15;
    GRACE_PERIOD_DAYS * 24 * 3600
}

pub const CLOCK_SKEW_SECS: i64 = 300;
pub const REFRESH_REQUIRED_WINDOW_SECS: i64 = 24 * 3600;
const ALLOWED_CHANNELS: &[&str] = &["stable", "beta", "nightly", "oss"];

pub fn validate_claim_semantics(
    claims: &crate::auth::state::auth_metadata::JwtClaims,
) -> Result<(), AuthError> {
    if claims.sub.trim().is_empty() {
        return Err(AuthError::JwtError(
            "JWT subject must not be empty".to_string(),
        ));
    }

    if claims.mid.trim().is_empty() {
        return Err(AuthError::JwtError(
            "JWT machine binding must not be empty".to_string(),
        ));
    }

    if !ALLOWED_CHANNELS.contains(&claims.channel.as_str()) {
        return Err(AuthError::JwtError(format!(
            "Unsupported build channel in JWT: {}",
            claims.channel
        )));
    }

    if claims.iat <= 0 {
        return Err(AuthError::JwtError(
            "JWT issued-at must be positive".to_string(),
        ));
    }

    if claims.exp <= 0 {
        return Err(AuthError::JwtError(
            "JWT expiration must be positive".to_string(),
        ));
    }

    if claims.gexp <= 0 {
        return Err(AuthError::JwtError(
            "JWT grace expiration must be positive".to_string(),
        ));
    }

    if claims.exp < claims.iat {
        return Err(AuthError::JwtError(
            "JWT expiration precedes issued-at".to_string(),
        ));
    }

    if claims.gexp < claims.exp {
        return Err(AuthError::JwtError(
            "JWT grace expiration precedes expiration".to_string(),
        ));
    }

    if claims.uexp < 0 {
        return Err(AuthError::JwtError(
            "JWT update entitlement must not be negative".to_string(),
        ));
    }

    if claims.uexp != 0 && claims.uexp < claims.exp {
        return Err(AuthError::JwtError(
            "JWT update entitlement expires before the license expires".to_string(),
        ));
    }

    let _tier = LicenseTier::from_str(&claims.tier)?;

    Ok(())
}

pub fn validate_jwt_timing(metadata: &JwtMetadata, now: i64) -> Result<(), AuthError> {
    if metadata.issued_at > now + CLOCK_SKEW_SECS {
        return Err(AuthError::JwtError(
            "JWT issued-at is too far in the future".to_string(),
        ));
    }

    if metadata.expires_at < metadata.issued_at {
        return Err(AuthError::JwtError(
            "JWT expiration precedes issued-at".to_string(),
        ));
    }

    if metadata.grace_expires_at < metadata.expires_at {
        return Err(AuthError::JwtError(
            "JWT grace expiration precedes expiration".to_string(),
        ));
    }

    if metadata.update_expires_at < 0 {
        return Err(AuthError::JwtError(
            "JWT update entitlement must not be negative".to_string(),
        ));
    }

    Ok(())
}

pub fn classify_launch_status(metadata: &JwtMetadata, now: i64) -> AuthStatus {
    if now >= metadata.expires_at {
        if now < metadata.grace_expires_at {
            return AuthStatus::GracePeriod;
        }
        return AuthStatus::Expired;
    }

    if metadata.expires_at - now <= REFRESH_REQUIRED_WINDOW_SECS {
        return AuthStatus::RefreshRequired;
    }

    AuthStatus::Valid
}

pub fn build_jwt_metadata(
    claims: crate::auth::state::auth_metadata::JwtClaims,
) -> Result<JwtMetadata, AuthError> {
    validate_claim_semantics(&claims)?;

    let tier = LicenseTier::from_str(&claims.tier)?;

    Ok(JwtMetadata {
        sub: claims.sub,
        tier,
        mid: claims.mid,
        channel: claims.channel,
        flags: claims.flags,
        issued_at: claims.iat,
        expires_at: claims.exp,
        grace_expires_at: claims.gexp,
        update_expires_at: claims.uexp,
    })
}

// ── Dev-auth path ─────────────────────────────────────────────────────────────
// All JWT generation and local-secret validation is ONLY compiled when the
// `dev-auth` feature is active. This makes it a compile-time impossibility for
// a production binary to accept dev-signed tokens.
#[cfg(feature = "dev-auth")]
mod dev {
    use super::*;
    use crate::auth::state::auth_metadata::JwtClaims;
    use chrono::Utc;

    const LOCAL_SIM_SECRET: &[u8] = b"aspectshift-local-dev-secret-do-not-use-in-production";
    const JWT_VALIDITY_DAYS: i64 = 30;

    pub fn generate_jwt(
        sub: &str,
        tier: &LicenseTier,
        machine_id: &str,
    ) -> Result<String, AuthError> {
        let now = Utc::now().timestamp();
        let exp = now + (JWT_VALIDITY_DAYS * 24 * 3600);
        let gexp = exp + (15 * 24 * 3600); // 15 day grace by default in dev

        let claims = JwtClaims {
            sub: sub.to_string(),
            tier: tier.as_str().to_string(),
            mid: machine_id.to_string(),
            channel: "stable".to_string(),
            flags: 0,
            iat: now,
            exp,
            gexp,
            uexp: exp, // update entitlement matches expiry in dev
            unknown_fields: std::collections::HashMap::new(),
        };

        let header = base64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload_json =
            serde_json::to_string(&claims).map_err(|e| AuthError::JwtError(e.to_string()))?;
        let payload = base64url_encode(payload_json.as_bytes());

        let signing_input = format!("{}.{}", header, payload);
        let signature = compute_hmac_sha256(LOCAL_SIM_SECRET, signing_input.as_bytes());
        let sig_encoded = base64url_encode(&signature);

        Ok(format!("{}.{}.{}", header, payload, sig_encoded))
    }

    pub fn validate_jwt(token: &str) -> Result<JwtMetadata, AuthError> {
        let parts: Vec<&str> = token.splitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(AuthError::TokenCorrupted);
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let expected_sig = compute_hmac_sha256(LOCAL_SIM_SECRET, signing_input.as_bytes());
        let expected_sig_encoded = base64url_encode(&expected_sig);

        if expected_sig_encoded != parts[2] {
            return Err(AuthError::TokenCorrupted);
        }

        let payload_bytes = base64url_decode(parts[1]).map_err(|_| AuthError::TokenCorrupted)?;
        let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
            .map_err(|e| AuthError::JwtError(e.to_string()))?;

        let metadata = build_jwt_metadata(claims)?;
        validate_jwt_timing(&metadata, chrono::Utc::now().timestamp())?;

        Ok(metadata)
    }

    fn compute_hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn base64url_encode(data: &[u8]) -> String {
        let standard = base64_encode(data);
        standard
            .replace('+', "-")
            .replace('/', "_")
            .replace('=', "")
    }

    fn base64url_decode(s: &str) -> Result<Vec<u8>, ()> {
        let padded = {
            let mut p = s.replace('-', "+").replace('_', "/");
            while p.len() % 4 != 0 {
                p.push('=');
            }
            p
        };
        base64_decode(&padded).map_err(|_| ())
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

    fn base64_decode(s: &str) -> Result<Vec<u8>, ()> {
        let s = s.as_bytes();
        let mut result = Vec::new();
        let mut i = 0;
        fn val(c: u8) -> Result<usize, ()> {
            match c {
                b'A'..=b'Z' => Ok((c - b'A') as usize),
                b'a'..=b'z' => Ok((c - b'a' + 26) as usize),
                b'0'..=b'9' => Ok((c - b'0' + 52) as usize),
                b'+' => Ok(62),
                b'/' => Ok(63),
                b'=' => Ok(0),
                _ => Err(()),
            }
        }
        while i + 3 < s.len() {
            let v0 = val(s[i])?;
            let v1 = val(s[i + 1])?;
            let v2 = val(s[i + 2])?;
            let v3 = val(s[i + 3])?;
            result.push(((v0 << 2) | (v1 >> 4)) as u8);
            if s[i + 2] != b'=' {
                result.push(((v1 << 4) | (v2 >> 2)) as u8);
            }
            if s[i + 3] != b'=' {
                result.push(((v2 << 6) | v3) as u8);
            }
            i += 4;
        }
        Ok(result)
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

#[cfg(feature = "dev-auth")]
pub use dev::{generate_jwt, validate_jwt};

#[cfg(not(feature = "dev-auth"))]
pub fn generate_jwt(
    _sub: &str,
    _tier: &LicenseTier,
    _machine_id: &str,
) -> Result<String, AuthError> {
    Err(AuthError::PhaseDNotImplemented)
}

#[cfg(not(feature = "dev-auth"))]
pub fn validate_jwt(token: &str) -> Result<JwtMetadata, AuthError> {
    crate::auth::crypto::jwt::validate_production_jwt(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::state::auth_metadata::{JwtClaims, JwtMetadata};

    fn sample_claims() -> JwtClaims {
        JwtClaims {
            sub: "subject-123".to_string(),
            tier: "pro".to_string(),
            mid: "mid_ABC123".to_string(),
            channel: "stable".to_string(),
            flags: 0,
            iat: 1_700_000_000,
            exp: 1_700_000_600,
            gexp: 1_700_001_200,
            uexp: 1_700_000_600,
            unknown_fields: std::collections::HashMap::new(),
        }
    }

    fn sample_metadata(now: i64) -> JwtMetadata {
        JwtMetadata {
            sub: "subject-123".to_string(),
            tier: LicenseTier::Pro,
            mid: "mid_ABC123".to_string(),
            channel: "stable".to_string(),
            flags: 0,
            issued_at: now - 10,
            expires_at: now + REFRESH_REQUIRED_WINDOW_SECS + 10_000,
            grace_expires_at: now + 20_000,
            update_expires_at: now + 20_000,
        }
    }

    #[test]
    fn claim_semantics_rejects_invalid_channel() {
        let mut claims = sample_claims();
        claims.channel = "preview".to_string();

        let result = validate_claim_semantics(&claims);
        assert!(
            matches!(result, Err(AuthError::JwtError(e)) if e.contains("Unsupported build channel"))
        );
    }

    #[test]
    fn missing_claims_are_rejected_during_deserialization() {
        let payload = serde_json::json!({
            "sub": "subject-123",
            "tier": "pro"
        });

        let result: Result<JwtClaims, _> = serde_json::from_value(payload);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_claims_are_accepted_and_flattened() {
        let payload = serde_json::json!({
            "sub": "subject-123",
            "tier": "pro",
            "mid": "mid_ABC123",
            "channel": "stable",
            "flags": 0,
            "iat": 1_700_000_000,
            "exp": 1_700_000_600,
            "gexp": 1_700_001_200,
            "uexp": 1_700_000_600,
            "future_claim": "test_value"
        });

        let claims: JwtClaims = serde_json::from_value(payload).expect("Should deserialize with unknown fields");
        assert_eq!(claims.sub, "subject-123");
        assert_eq!(claims.unknown_fields.get("future_claim").and_then(|v| v.as_str()), Some("test_value"));
    }

    #[test]
    fn timing_rejects_future_issued_tokens_beyond_skew() {
        let metadata = JwtMetadata {
            sub: "subject-123".to_string(),
            tier: LicenseTier::Pro,
            mid: "mid_ABC123".to_string(),
            channel: "stable".to_string(),
            flags: 0,
            issued_at: 10_000,
            expires_at: 20_000,
            grace_expires_at: 30_000,
            update_expires_at: 20_000,
        };

        let result = validate_jwt_timing(&metadata, 1_000);
        assert!(matches!(result, Err(AuthError::JwtError(e)) if e.contains("future")));
    }

    #[test]
    fn classify_launch_status_transitions_are_deterministic() {
        let now = 1_700_000_000;

        let mut valid = sample_metadata(now);
        assert_eq!(classify_launch_status(&valid, now), AuthStatus::Valid);

        valid.expires_at = now + REFRESH_REQUIRED_WINDOW_SECS - 1;
        assert_eq!(
            classify_launch_status(&valid, now),
            AuthStatus::RefreshRequired
        );

        valid.expires_at = now - 1;
        valid.grace_expires_at = now + 100;
        assert_eq!(classify_launch_status(&valid, now), AuthStatus::GracePeriod);

        valid.grace_expires_at = now - 1;
        assert_eq!(classify_launch_status(&valid, now), AuthStatus::Expired);
    }
}
