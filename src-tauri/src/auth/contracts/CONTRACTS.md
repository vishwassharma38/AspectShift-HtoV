# Licensing Backend Contract

- **Frozen on**: June 1, 2026
- **Wire format**: JSON over HTTPS for API routes, camelCase field names in JSON bodies and responses; Gumroad webhooks are accepted as `application/x-www-form-urlencoded` with Gumroad's snake_case form keys
- **Runtime**: all implemented API routes are `POST` handlers and run in the Edge runtime
- **Authentication**:
  - `/api/activate` uses license-key authentication in the request body
  - `/api/refresh` and `/api/updates/check` require a JWT in the request body, not a header
  - `/api/gumroad/webhook` requires a valid `seller_id` that matches `GUMROAD_SELLER_ID`
- **Versioning policy**: the current API paths are unversioned; breaking changes require a new path
- **JWT algorithm**: EdDSA (`Ed25519`) only; there is no HS256 fallback in this codebase
- **Rate limiting**: none is implemented in the current codebase

## API Endpoints

### `POST /api/activate`

Request body:

```json
{
  "licenseKey": "string",
  "machineId": "string",
  "appVersion": "string",
  "channel": "stable | beta | nightly | oss"
}
```

Behavior:

- Trims all string fields before validation.
- Rejects empty `licenseKey`, `machineId`, or `appVersion` with `400 INVALID_REQUEST`.
- Rejects an unknown `channel` with `400 INVALID_REQUEST`.
- Looks up the license by `gumroad_license_key`.
- Returns `404 LICENSE_NOT_FOUND` when no license row exists.
- Returns `401 LICENSE_REVOKED` or `401 LICENSE_REFUNDED` when the license is blocked.
- Reuses an existing non-revoked activation for the same `machineId` when one exists.
- Otherwise counts non-revoked activations for the license and returns `409 ACTIVATION_LIMIT_REACHED` when the limit is reached.
- Creates a new activation row with `is_revoked = false` and `grace_state = "active"` when needed.
- On reuse, updates `app_version`, `build_channel`, `last_active_at`, and `last_refresh_at`.
- On success, issues a fresh JWT and returns its ISO expiry time.

Success response:

```json
{
  "ok": true,
  "token": "string",
  "expiresAt": "RFC3339 timestamp"
}
```

Error response:

```json
{
  "ok": false,
  "error": "INVALID_REQUEST | LICENSE_NOT_FOUND | LICENSE_REVOKED | LICENSE_REFUNDED | ACTIVATION_LIMIT_REACHED | SERVER_ERROR",
  "message": "string"
}
```

Notes:

- Success JWT claims are built from the current license row, the requested machine id, and the normalized channel.
- The issued token lifetime is 7 days.
- `gexp` is 15 days after `exp`.
- `uexp` comes from `licenses.update_entitlement_expires_at` when present; otherwise it falls back to `exp`.
- `flags` is currently always `0`.

### `POST /api/refresh`

Request body:

```json
{
  "token": "string",
  "machineId": "string"
}
```

Behavior:

- Rejects malformed JSON, non-object JSON, empty fields, or missing fields with `400 INVALID_REQUEST`.
- Verifies the JWT with the configured public key and `JWT_KID`.
- Rejects malformed, expired, tampered, or wrong-key JWTs with `401 INVALID_TOKEN`.
- Requires `payload.mid` to match the request body `machineId`; otherwise returns `403 MACHINE_MISMATCH`.
- Re-looks up the license by `payload.sub`.
- Treats a missing license row as `403 LICENSE_REVOKED`.
- Rejects revoked or refunded licenses with `403 LICENSE_REVOKED` or `403 LICENSE_REFUNDED`.
- Validates the activation row for `(license_id, machine_id)` and requires `is_revoked = false`.
- Returns `403 ACTIVATION_REVOKED` when the activation is missing or revoked.
- Rebuilds JWT claims using the license row and the canonical activation channel.
- Uses `activations.build_channel` when present; otherwise falls back to the token's `channel` for legacy rows.
- Updates the activation heartbeat by setting `last_active_at` and `last_refresh_at` to the current server time.

Success response:

```json
{
  "ok": true,
  "token": "string",
  "expiresAt": "RFC3339 timestamp"
}
```

Error response:

```json
{
  "error": "INVALID_REQUEST | INVALID_TOKEN | LICENSE_REVOKED | LICENSE_REFUNDED | MACHINE_MISMATCH | ACTIVATION_REVOKED | SERVER_ERROR"
}
```

Notes:

- `refresh` does not enforce a channel allowlist.
- `refresh` keeps the machine binding strict: the JWT and request body must refer to the same machine.

### `POST /api/updates/check`

Request body:

```json
{
  "token": "string",
  "currentVersion": "x.y.z"
}
```

Behavior:

- Rejects malformed JSON, non-object JSON, missing fields, empty fields, or non-strict-semver `currentVersion` values with `400 INVALID_REQUEST`.
- Verifies the JWT with the configured public key and `JWT_KID`.
- Rejects malformed, expired, tampered, or wrong-key JWTs with `401 INVALID_TOKEN`.
- Re-looks up the license by `payload.sub`.
- Treats a missing license row as `403 LICENSE_REVOKED`.
- Rejects revoked or refunded licenses with `403 LICENSE_REVOKED` or `403 LICENSE_REFUNDED`.
- Validates the activation row for `(license_id, payload.mid)` and requires `is_revoked = false`.
- Returns `403 ACTIVATION_REVOKED` when the activation is missing or revoked.
- Uses `activations.build_channel` when present; otherwise falls back to the token's `channel`.
- Rejects the `oss` channel with `403 CHANNEL_NOT_ALLOWED`.
- Checks update entitlement against `update_entitlements`.
- Returns `403 UPDATES_NOT_ENTITLED` when the entitlement is missing, expired, or not perpetual.
- Resolves release targets from `UPDATES_STABLE_*`, `UPDATES_BETA_*`, or `UPDATES_NIGHTLY_*` depending on the canonical channel.
- Requires `manifestUrl` to be HTTPS and to match one of the comma-separated origins in `UPDATES_TRUSTED_MANIFEST_ORIGINS`.
- Throws `SERVER_ERROR` when release configuration is invalid or untrusted.
- Returns `{ allowed: false }` when the current version is already at or above the configured latest version.

Success response when an update is available:

```json
{
  "allowed": true,
  "latestVersion": "x.y.z",
  "manifestUrl": "https://example.com/manifest",
  "rollbackVersion": "x.y.z"
}
```

Success response when no update is available:

```json
{
  "allowed": false
}
```

Error response:

```json
{
  "error": "INVALID_REQUEST | INVALID_TOKEN | LICENSE_REVOKED | LICENSE_REFUNDED | ACTIVATION_REVOKED | UPDATES_NOT_ENTITLED | CHANNEL_NOT_ALLOWED | SERVER_ERROR"
}
```

### `POST /api/gumroad/webhook`

Request requirements:

- `Content-Type` must be `application/x-www-form-urlencoded`.
- The body must parse as a non-empty form payload.
- `seller_id` must match `GUMROAD_SELLER_ID` using constant-time comparison.
- Test events are accepted and ignored.

Normalized payload fields:

- `sellerId`
- `saleId`
- `orderNumber`
- `productId`
- `licenseKey`
- `email`
- `subscriptionId`
- `purchaseIds`
- `resourceName`
- `cancelled`
- `ended`
- `restarted`
- `recurringCharge`
- `cancelledAt`
- `endedAt`
- `restartedAt`
- `endedReason`
- `subscriptionEndedAt`
- `subscriptionCancelledAt`
- `subscriptionFailedAt`
- `chargeOccurrenceCount`
- `refunded`
- `disputed`
- `disputeWon`
- `saleTimestamp`
- `isTest`
- `discoverFeeCharged`
- `canContact`
- `raw`

Event routing:

- `purchase`
  - Requires `sale_id`, `order_number`, `product_id`, `license_key`, `email`, and `sale_timestamp`.
  - Unknown product ids are ignored with a 200 response.
  - Known tier products create or update `licenses` and `update_entitlements`, then persist `gumroad_purchase_id`.
- `refund`
  - Marks the license referenced by `gumroad_purchase_id` as refunded.
- `dispute`
  - Marks the license referenced by `gumroad_purchase_id` as refunded.
- `dispute_won`
  - Clears the refunded flag for the matching license.
- `subscription_purchase`, `subscription_cancel`, `subscription_renewal`, `subscription_restart`, `subscription_expired`
  - Route through the subscription entitlement workflow.
  - Unknown subscription product ids are ignored with a 200 response.
  - The workflow is idempotent and replay-aware using the stored `subscription_entitlements.metadata`.
- Unsupported event types are ignored and still return 200.

Success response:

```json
{
  "ok": true
}
```

Error response:

```json
{
  "error": "INVALID_REQUEST | UNAUTHORIZED_WEBHOOK | SERVER_ERROR"
}
```

## `ProductionJwtClaims`

All JWTs issued by the server use these claims:

| Field     | Type                                       | Meaning                                                 |
| :-------- | :----------------------------------------- | :------------------------------------------------------ |
| `sub`     | `string`                                   | Internal Supabase license id                            |
| `tier`    | `"community" \| "pro"`                     | License tier                                            |
| `mid`     | `string`                                   | Machine id bound to the token                           |
| `channel` | `"stable" \| "beta" \| "nightly" \| "oss"` | Build channel                                           |
| `flags`   | `integer`                                  | Feature flag bitmask, currently always `0`              |
| `iat`     | `integer`                                  | Issued-at time in Unix seconds                          |
| `exp`     | `integer`                                  | Expiry time in Unix seconds, 7 days after `iat`         |
| `gexp`    | `integer`                                  | Grace expiry time in Unix seconds, 15 days after `exp` |
| `uexp`    | `integer`                                  | Update entitlement expiry in Unix seconds               |

Validation rules:

- `sub`, `tier`, `mid`, `channel`, `flags`, `iat`, `exp`, `gexp`, and `uexp` are all required.
- All timestamps must be non-negative integers.
- `tier` must be `community` or `pro`.
- JWT verification enforces `alg = EdDSA` and the configured `kid`.

## Supabase Table Shapes

### `licenses`

- `id` (`uuid`, primary key)
- `gumroad_license_key` (`text`, unique)
- `gumroad_purchase_id` (`text`, nullable)
- `email` (`text`, nullable)
- `tier` (`text`) - `community` or `pro`
- `activation_limit` (`integer`)
- `is_refunded` (`boolean`)
- `is_revoked` (`boolean`)
- `update_entitlement_expires_at` (`timestamp`, nullable)
- `created_at` (`timestamp`)
- `updated_at` (`timestamp`)

Notes:

- New purchases can create licenses with `gumroad_purchase_id = null` until the purchase metadata is persisted.
- `update_entitlement_expires_at` is copied into the issued JWT as `uexp` when present.

### `activations`

- `id` (`uuid`, primary key)
- `license_id` (`uuid`, foreign key to `licenses.id`)
- `machine_id` (`text`)
- `app_version` (`text`)
- `build_channel` (`text`)
- `last_active_at` (`timestamp`)
- `last_refresh_at` (`timestamp`)
- `is_revoked` (`boolean`)
- `grace_state` (`text`) - `active`, `in_grace`, or `expired`
- `created_at` (`timestamp`)

Notes:

- The current code only writes `grace_state = "active"` when creating an activation.
- Refresh and activate reuse rows only when `is_revoked = false`.

### `update_entitlements`

- `id` (`uuid`, primary key)
- `license_id` (`uuid`, foreign key to `licenses.id`)
- `entitled_until` (`timestamp`)
- `is_perpetual` (`boolean`)
- `grandfathered_below_version` (`text`, nullable)
- `created_at` (`timestamp`)
- `updated_at` (`timestamp`)

Notes:

- `isUpdateEntitled` reads this table to decide whether `/api/updates/check` can return an update.
- A perpetual entitlement bypasses the timestamp check.

### `subscription_entitlements`

- `id` (`uuid`, primary key)
- `license_id` (`uuid`, foreign key to `licenses.id`)
- `feature_key` (`text`)
- `gumroad_subscription_id` (`text`, nullable)
- `gumroad_purchase_id` (`text`, unique, nullable)
- `product_id` (`text`)
- `status` (`text`) - `active`, `cancelled`, or `expired`
- `starts_at` (`timestamp`)
- `entitled_until` (`timestamp`, nullable)
- `cancelled_at` (`timestamp`, nullable)
- `metadata` (`jsonb`, default `{}`)
- `created_at` (`timestamp`)
- `updated_at` (`timestamp`)

Constraints:

- `(license_id, feature_key)` is unique.
- `gumroad_purchase_id` is unique.

Metadata used by the current subscription workflow:

- `feature_key`
- `billing`
- `product_id`
- `subscription_id`
- `purchase_id`
- `last_event_type`
- `last_event_timestamp`
- `last_charge_occurrence_count`

## Environment Variables

### Core runtime

- `SUPABASE_URL` - required for database access
- `SUPABASE_SERVICE_ROLE_KEY` - required for database access
- `JWT_KID` - required JWT key id
- `JWT_PRIVATE_KEY` - required inline PKCS8 private key PEM
- `JWT_PUBLIC_KEY` - required inline SPKI public key PEM

### Gumroad webhook authentication

- `GUMROAD_SELLER_ID` - required to accept webhook calls

### Gumroad tier products

- `GUMROAD_PRODUCT_COMMUNITY`
- `GUMROAD_PRODUCT_PRO`
- `GUMROAD_PRODUCT_IDS` - optional comma-separated allowlist for tier products
- `GUMROAD_DEFAULT_ACTIVATION_LIMIT`
- `GUMROAD_COMMUNITY_ACTIVATION_LIMIT`
- `GUMROAD_PRO_ACTIVATION_LIMIT`
- `GUMROAD_COMMUNITY_UPDATE_ENTITLEMENT_MODE`
- `GUMROAD_PRO_UPDATE_ENTITLEMENT_MODE`
- `GUMROAD_COMMUNITY_UPDATE_ENTITLEMENT_DAYS`
- `GUMROAD_PRO_UPDATE_ENTITLEMENT_DAYS`
- `GUMROAD_COMMUNITY_GRANDFATHERED_BELOW_VERSION`
- `GUMROAD_PRO_GRANDFATHERED_BELOW_VERSION`

### Gumroad subscription products

- `GUMROAD_SUB_LUT_PACKS_PRODUCT`
- `GUMROAD_SUB_CLOUD_BACKUP_PRODUCT`
- `GUMROAD_SUB_AI_PROCESSING_PRODUCT`
- `GUMROAD_SUB_CREATOR_ASSETS_PRODUCT`
- `GUMROAD_SUB_PREMIUM_TEMPLATES_PRODUCT`
- `GUMROAD_SUB_PRODUCT_IDS` - optional comma-separated allowlist for subscription products
- `GUMROAD_SUB_LUT_PACKS_BILLING`
- `GUMROAD_SUB_CLOUD_BACKUP_BILLING`
- `GUMROAD_SUB_AI_PROCESSING_BILLING`
- `GUMROAD_SUB_CREATOR_ASSETS_BILLING`
- `GUMROAD_SUB_PREMIUM_TEMPLATES_BILLING`

### Update check release targets

- `UPDATES_STABLE_LATEST_VERSION`
- `UPDATES_STABLE_MANIFEST_URL`
- `UPDATES_STABLE_ROLLBACK_VERSION`
- `UPDATES_BETA_LATEST_VERSION`
- `UPDATES_BETA_MANIFEST_URL`
- `UPDATES_BETA_ROLLBACK_VERSION`
- `UPDATES_NIGHTLY_LATEST_VERSION`
- `UPDATES_NIGHTLY_MANIFEST_URL`
- `UPDATES_NIGHTLY_ROLLBACK_VERSION`
- `UPDATES_TRUSTED_MANIFEST_ORIGINS` - comma-separated HTTPS origins allowed for manifest URLs

Notes:

- The update-check route rejects manifest URLs that are not HTTPS or that do not match a trusted origin.
- `oss` is not a valid release target and is always denied by `/api/updates/check`.
