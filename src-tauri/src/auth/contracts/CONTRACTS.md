# Licensing Backend Data Contracts

- **Frozen on**: May 26, 2026
- **Wire format**: JSON over HTTPS, camelCase field names (enforced by `#[serde(rename_all = "camelCase")]`)
- **Authentication**: all requests to `/refresh` and `/updates/check` include the JWT in the request body (not a header) to keep edge function parsing simple
- **Versioning policy**: the `/activate`, `/refresh`, and `/updates/check` endpoints are unversioned for now; breaking changes require a new endpoint path
- **JWT algorithm**: EdDSA (Ed25519) in production — the edge function signs with a private key, the Tauri app verifies with a bundled public key. The current dev-auth feature uses HS256 with a local secret as a development stand-in only.

## `ProductionJwtClaims` field table

| Field     | Type     | Description                                             | Local Validation | Network Required |
| :-------- | :------- | :------------------------------------------------------ | :--------------- | :--------------- |
| `sub`     | `String` | Subject: internal license ID from Supabase              | Yes              | No               |
| `tier`    | `String` | License tier: "community" \| "pro"                      | Yes              | No               |
| `mid`     | `String` | Machine fingerprint hash                                | Yes              | No               |
| `channel` | `String` | Build channel: "stable" \| "beta" \| "nightly" \| "oss" | Yes              | No               |
| `flags`   | `u32`    | Feature flags as a compact bitmask                      | Yes              | No               |
| `iat`     | `i64`    | Issued-at: Unix timestamp seconds                       | Yes              | No               |
| `exp`     | `i64`    | Expires-at: Unix timestamp seconds                      | Yes              | No               |
| `gexp`    | `i64`    | Grace-expires-at: Unix timestamp seconds                | Yes              | No               |
| `uexp`    | `i64`    | Update entitlement expiry: Unix timestamp seconds       | Yes              | No               |

## Supabase table shapes

### `licenses`

- `id` (uuid, primary key)
- `gumroad_license_key` (text, unique)
- `gumroad_purchase_id` (text)
- `email` (text)
- `tier` (text) - "community" | "pro"
- `activation_limit` (integer)
- `is_refunded` (boolean)
- `is_revoked` (boolean)
- `update_entitlement_expires_at` (timestamp, nullable)
- `created_at` (timestamp)
- `updated_at` (timestamp)

### `activations`

- `id` (uuid, primary key)
- `license_id` (uuid, foreign key to `licenses.id`)
- `machine_id` (text)
- `app_version` (text)
- `build_channel` (text)
- `last_active_at` (timestamp)
- `last_refresh_at` (timestamp)
- `is_revoked` (boolean)
- `grace_state` (text) - "active" | "in_grace" | "expired"
- `created_at` (timestamp)

### `update_entitlements`

- `id` (uuid, primary key)
- `license_id` (uuid, foreign key to `licenses.id`)
- `entitled_until` (timestamp)
- `is_perpetual` (boolean)
- `grandfathered_below_version` (text, nullable)
- `created_at` (timestamp)
- `updated_at` (timestamp)

## What is NOT frozen yet

- Gumroad webhook payload shape
- Supabase row-level security policies
