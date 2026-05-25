use chrono::Utc;

use crate::auth::auth_errors::AuthError;
use crate::auth::state::auth_metadata::{JwtClaims, JwtMetadata};
use crate::auth::state::license_tier::LicenseTier;

const LOCAL_SIM_SECRET: &[u8] = b"aspectshift-local-dev-secret-do-not-use-in-production";

const JWT_VALIDITY_DAYS: i64 = 30;
const GRACE_PERIOD_DAYS: i64 = 7;

pub fn generate_jwt(sub: &str, tier: &LicenseTier, machine_id: &str) -> Result<String, AuthError> {
    let now = Utc::now().timestamp();
    let exp = now + (JWT_VALIDITY_DAYS * 24 * 3600);

    let claims = JwtClaims {
        sub: sub.to_string(),
        tier: tier.as_str().to_string(),
        mid: machine_id.to_string(),
        iat: now,
        exp,
    };

    let header = base64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload_json = serde_json::to_string(&claims).map_err(|e| AuthError::JwtError(e.to_string()))?;
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
    let claims: JwtClaims =
        serde_json::from_slice(&payload_bytes).map_err(|e| AuthError::JwtError(e.to_string()))?;

    let tier = LicenseTier::from_str(&claims.tier);

    Ok(JwtMetadata {
        sub: claims.sub,
        tier,
        mid: claims.mid,
        issued_at: claims.iat,
        expires_at: claims.exp,
    })
}

pub fn grace_period_secs() -> i64 {
    GRACE_PERIOD_DAYS * 24 * 3600
}

fn compute_hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
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


