# AspectShift-HtoV Licensing Backend — Frozen Contract

- **Frozen on**: June 4, 2026
- **Supersedes**: Contract frozen June 1, 2026
- **Wire format**: JSON over HTTPS for all API routes; camelCase field names in JSON bodies and responses; Gumroad webhooks are accepted as `application/x-www-form-urlencoded` with Gumroad's snake_case form keys
- **Runtime**: all implemented API routes are `POST` handlers and run in the Edge runtime
- **Authentication**:
  - `/api/activate` uses license-key authentication in the request body
  - `/api/refresh` and `/api/updates/check` require a JWT in the request body, not a header
  - `/api/gumroad/webhook` requires a valid `seller_id` that matches `GUMROAD_SELLER_ID`
- **Versioning policy**: the current API paths are unversioned; breaking changes require a new path
- **JWT algorithm**: EdDSA (`Ed25519`) only; there is no HS256 fallback in this codebase
- **Rate limiting**: none is implemented in the current codebase

---

## Bug-Fix Changelog (v2 — June 4, 2026)

This section documents every contract-level change introduced by this revision. Codex must enforce all items below on both the client and server.

| ID    | Severity | Bug                                                                                                                 | Resolution                                                                                                                                                                                                |
| :---- | :------- | :------------------------------------------------------------------------------------------------------------------ | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| BF-01 | HIGH     | Grace Refresh Split — `/api/refresh` rejected a JWT as expired before grace logic could run                         | `/api/refresh` now accepts any token whose `gexp` has not yet elapsed. The server MUST check `gexp`, not only `exp`. See [§ Refresh — Behavior](#post-apirefresh).                                        |
| BF-02 | HIGH     | Stale Update Contract Drift — client and server used inconsistent request/response schemas for `/api/updates/check` | The canonical `/api/updates/check` request and response shapes are now defined in one place in this document. Both sides MUST derive their schemas from this section only.                                |
| BF-03 | MEDIUM   | Write-Only `grace_state` — runtime only wrote `active`; `in_grace` and `expired` were dead code                     | `grace_state` transitions are now fully specified and both sides MUST implement them. See [§ grace_state Lifecycle](#grace_state-lifecycle).                                                              |
| BF-04 | MEDIUM   | Client-Only Clock Skew Policy — client tolerated 300 s future `iat`; server did not                                 | A shared ±300 s clock-skew tolerance is now mandated for both sides. See [§ JWT Validation Rules](#jwt-validation-rules).                                                                                 |
| BF-05 | MEDIUM   | Refresh Eligibility Contradiction — client allowed refresh during grace; server only allowed it pre-expiry          | Refresh eligibility is now frozen at: allowed while `now ≤ gexp`. Both sides MUST enforce this identical rule. See [§ Token Lifecycle and Refresh Eligibility](#token-lifecycle-and-refresh-eligibility). |
| BF-06 | MEDIUM   | Reconnect Self-Heal Failure — users could not recover automatically when `exp < now ≤ gexp`                         | Covered by BF-01 and BF-05. The server now accepts grace-period tokens on `/api/refresh`, enabling automatic self-healing reconnect.                                                                      |

---

## Ownership and Responsibility Boundaries

This section is normative. Any behavior, validation, state management, security rule, or business logic listed here MUST be implemented on the designated side only. Codex MUST flag any implementation of server-side responsibilities on the client and vice versa.

### Server-Side Responsibilities (License Server only)

- All JWT issuance, signing, and EdDSA key management
- All JWT verification (signature, `alg`, `kid`, expiry relative to `gexp`, claim completeness)
- Clock-skew tolerance enforcement during JWT verification (±300 s on `iat`)
- License row lookup, status checks (revoked, refunded), and activation counting
- Activation row creation, reuse, and heartbeat updates (`last_active_at`, `last_refresh_at`)
- `grace_state` transition writes (`active` → `in_grace` → `expired`)
- Update entitlement evaluation (`update_entitlements` table)
- Channel allowlist enforcement (rejecting `oss` on `/api/updates/check`)
- Gumroad webhook signature/seller verification and event routing
- Rate limiting (if/when implemented)
- All Supabase table writes

### Client-Side Responsibilities (Tauri Application only)

- Secure, persistent storage of the JWT and `expiresAt` / `gracePeriodEndsAt` values
- Determining the current token state (valid, in-grace, expired) from the JWT claims `exp` and `gexp` (canonical source of truth; see [§ Token Lifecycle and Refresh Eligibility](#token-lifecycle-and-refresh-eligibility))
- Deciding when to call `/api/refresh` proactively (see [§ Token Lifecycle and Refresh Eligibility](#token-lifecycle-and-refresh-eligibility))
- Displaying grace-period warnings and expired-license UI states
- Constructing well-formed request bodies (camelCase JSON, correct field names)
- Generating and persisting a stable `machineId`
- Reporting `appVersion` and `channel` accurately
- Surfacing actionable error messages to the user for each server error code

### Shared Responsibilities (both sides must implement independently)

- Clock-skew tolerance: both sides tolerate ±300 s on `iat` when evaluating token freshness
- Token state classification: both sides MUST agree on the definition of valid / in-grace / expired based solely on `exp`, `gexp`, and the current time (see [§ Token Lifecycle and Refresh Eligibility](#token-lifecycle-and-refresh-eligibility))

---

## Token Lifecycle and Refresh Eligibility

This section is the single source of truth for token state and refresh eligibility. Both the client and the server MUST derive their behavior from these definitions.

### Token States

Given `now` = current Unix time in seconds and the JWT claims `exp` and `gexp`:

| State      | Condition          | Description                                                                                                                        |
| :--------- | :----------------- | :--------------------------------------------------------------------------------------------------------------------------------- |
| `valid`    | `now ≤ exp`        | Token is fully active. All endpoints accept it.                                                                                    |
| `in_grace` | `exp < now ≤ gexp` | Token has expired but the grace window is open. `/api/refresh` accepts it. All other endpoints reject it with `401 INVALID_TOKEN`. |
| `expired`  | `now > gexp`       | Grace window has closed. No endpoint accepts this token. The user must re-activate.                                                |

### Refresh Eligibility Rule (Canonical)

> **A token is eligible for refresh if and only if `now ≤ gexp`.**

This defines **lifecycle eligibility only**. Lifecycle eligibility is determined solely by `now ≤ gexp`; normal license, activation, revocation, and machine-binding validation still applies.

- The server MUST NOT reject a token on `/api/refresh` solely because `now > exp`, as long as `now ≤ gexp`.
- The client MUST attempt a refresh call whenever the token is `in_grace` and network access is available.
- The client SHOULD attempt a proactive refresh when `now > exp − 86400` (i.e. within 24 hours of expiry) to avoid entering the grace window unnecessarily.

### Self-Heal Reconnect (BF-06)

When a user reconnects after an offline period and their token is `in_grace`:

1. The client MUST automatically call `/api/refresh` without requiring user interaction.
2. On success, the client MUST store the new token and update `exp`/`gexp` from the response.
3. On failure with `401 INVALID_TOKEN` (meaning `gexp` has also elapsed), the client MUST show an expired-license prompt and direct the user to re-activate.

---

## grace_state Lifecycle

This section is the single source of truth for `grace_state` transitions. The server MUST write these transitions. The client observes `grace_state` indirectly through JWT claims; it does not write to this field.

### States

| Value      | Meaning                                           |
| :--------- | :------------------------------------------------ |
| `active`   | Token has not yet expired (`now ≤ exp`)           |
| `in_grace` | Token is in the grace window (`exp < now ≤ gexp`) |
| `expired`  | Grace window has closed (`now > gexp`)            |

### Transition Rules (Server)

- **On activation or reuse**: write `grace_state = "active"`.
- **On `/api/refresh` where `exp < now ≤ gexp`**: write `grace_state = "in_grace"` for the activation row before issuing the new token. After issuing the new token (which resets `exp` and `gexp`), immediately write `grace_state = "active"` on the same row.
- **On any request where `now > gexp`**: if the activation row still shows `active` or `in_grace`, write `grace_state = "expired"`. This is a best-effort cleanup step and does not affect the primary error response.
- **`grace_state` reflects the currently valid JWT lifecycle and may be corrected during refresh if stale.** JWT claims (`exp`, `gexp`) are the authoritative source of truth for token lifecycle; `grace_state` is best-effort mirrored metadata. A stale `expired` row MUST NOT cause `/api/refresh` to fail when `exp < now ≤ gexp`; the server MUST correct the stale row and proceed with token issuance normally.
- **No other transitions are valid.** A row MUST NOT move from `expired` back to `active` or `in_grace` except through a fresh `/api/activate` call (which creates a new activation row) or through the stale-row correction described above during a valid grace-period refresh.

---

## JWT Validation Rules

These rules apply to **every** endpoint that accepts a JWT. Both the client (for local state decisions) and the server (for request verification) MUST implement them identically.

### Signature and Header

- Algorithm MUST be `EdDSA` (`Ed25519`). Tokens with any other `alg` MUST be rejected.
- `kid` MUST match `JWT_KID`. Tokens with a missing or mismatched `kid` MUST be rejected.

### Required Claims

All of the following claims MUST be present and well-typed. A token missing any of them MUST be rejected.

| Claim     | Type    | Constraint                                    |
| :-------- | :------ | :-------------------------------------------- |
| `sub`     | string  | non-empty                                     |
| `tier`    | string  | `"community"` or `"pro"`                      |
| `mid`     | string  | non-empty                                     |
| `channel` | string  | `"stable"`, `"beta"`, `"nightly"`, or `"oss"` |
| `flags`   | integer | ≥ 0                                           |
| `iat`     | integer | ≥ 0                                           |
| `exp`     | integer | ≥ 0, > `iat`                                  |
| `gexp`    | integer | ≥ 0, > `exp`                                  |
| `uexp`    | integer | ≥ 0                                           |

### Clock-Skew Tolerance (BF-04)

Both the client and server MUST apply a ±300 s (5-minute) tolerance when evaluating `iat`. A token whose `iat` is up to 300 s in the future relative to local clock MUST be accepted; a token whose `iat` is more than 300 s in the future MUST be rejected.

Expiry checks (`exp`, `gexp`) do NOT use skew tolerance; they are evaluated against the unmodified current time.

### Expiry Evaluation

- On `/api/refresh`: reject only when `now > gexp` (see BF-01, BF-05).
- On all other endpoints: reject when `now > exp`.

---

## API Endpoints

### `POST /api/activate`

**Request body:**

```json
{
  "licenseKey": "string",
  "machineId": "string",
  "appVersion": "string",
  "channel": "stable | beta | nightly | oss"
}
```

**Server behavior:**

- Trims all string fields before validation.
- Rejects empty `licenseKey`, `machineId`, or `appVersion` with `400 INVALID_REQUEST`.
- Rejects an unknown `channel` with `400 INVALID_REQUEST`.
- Looks up the license by `gumroad_license_key`.
- Returns `404 LICENSE_NOT_FOUND` when no license row exists.
- Returns `401 LICENSE_REVOKED` or `401 LICENSE_REFUNDED` when the license is blocked.
- Reuses an existing non-revoked activation for the same `machineId` when one exists.
- Otherwise counts non-revoked activations for the license and returns `409 ACTIVATION_LIMIT_REACHED` when the limit is reached.
- Creates a new activation row with `is_revoked = false` and `grace_state = "active"` when needed.
- On reuse, updates `app_version`, `build_channel`, `last_active_at`, `last_refresh_at`, and resets `grace_state = "active"`.
- On success, issues a fresh JWT and returns its ISO expiry times.

**Success response:**

```json
{
  "ok": true,
  "token": "string",
  "expiresAt": "RFC3339 timestamp",
  "gracePeriodEndsAt": "RFC3339 timestamp"
}
```

**Error response:**

```json
{
  "ok": false,
  "error": "INVALID_REQUEST | LICENSE_NOT_FOUND | LICENSE_REVOKED | LICENSE_REFUNDED | ACTIVATION_LIMIT_REACHED | SERVER_ERROR",
  "message": "string"
}
```

**JWT issuance notes:**

- Claims are built from the current license row, the requested `machineId`, and the normalized `channel`.
- Issued token lifetime (`exp`): 30 days after `iat`.
- Grace expiry (`gexp`): 15 days after `exp` (i.e. 45 days after `iat`).
- Update expiry (`uexp`): taken from `licenses.update_entitlement_expires_at` when present; otherwise the runtime falls back to `exp`.
- `flags` is always `0`.

**Client responsibility:**

- Store the issued JWT and treat its `exp` and `gexp` claims as the canonical source of truth for token lifecycle.
- `expiresAt` and `gracePeriodEndsAt` in the response are mirrored convenience fields that MUST match the JWT claims. Store them for convenience, but token state classification MUST derive from the JWT `exp` and `gexp` values. If a discrepancy is ever detected, JWT claims win.

---

### `POST /api/refresh`

**Request body:**

```json
{
  "token": "string",
  "machineId": "string"
}
```

**Server behavior:**

- Rejects malformed JSON, non-object JSON, empty fields, or missing fields with `400 INVALID_REQUEST`.
- Verifies the JWT signature, `alg`, and `kid`.
- Rejects malformed, tampered, or wrong-key JWTs with `401 INVALID_TOKEN`.
- **Rejects tokens where `now > gexp` with `401 INVALID_TOKEN`.** Tokens where `exp < now ≤ gexp` MUST be accepted (BF-01, BF-05).
- Requires `payload.mid` to match the request body `machineId`; otherwise returns `403 MACHINE_MISMATCH`.
- Re-looks up the license by `payload.sub`.
- Treats a missing license row as `403 LICENSE_REVOKED`.
- Rejects revoked or refunded licenses with `403 LICENSE_REVOKED` or `403 LICENSE_REFUNDED`.
- Validates the activation row for `(license_id, machine_id)` and requires `is_revoked = false`.
- Returns `403 ACTIVATION_REVOKED` when the activation is missing or revoked.
- If the incoming token was `in_grace` (`exp < now ≤ gexp`), writes `grace_state = "in_grace"` to the activation row before issuing the new token. A stale `grace_state = "expired"` on the activation row MUST NOT cause this step to fail; the server MUST overwrite stale state and proceed.
- Rebuilds JWT claims using the license row and the canonical activation channel.
- Uses `activations.build_channel` when present; otherwise falls back to the token's `channel` for legacy rows.
- Updates the activation heartbeat: sets `last_active_at` and `last_refresh_at` to the current server time.
- After issuing the new token, writes `grace_state = "active"` to the activation row.

**Success response:**

```json
{
  "ok": true,
  "token": "string",
  "expiresAt": "RFC3339 timestamp",
  "gracePeriodEndsAt": "RFC3339 timestamp"
}
```

**Error response:**

```json
{
  "ok": false,
  "error": "INVALID_REQUEST | INVALID_TOKEN | LICENSE_REVOKED | LICENSE_REFUNDED | MACHINE_MISMATCH | ACTIVATION_REVOKED | SERVER_ERROR"
}
```

**Notes:**

- `/api/refresh` does not enforce a channel allowlist.
- `/api/refresh` keeps machine binding strict: the JWT `mid` and the request body `machineId` MUST match.
- The re-issued token has a new `iat`, `exp` (30 days), and `gexp` (45 days from new `iat`).

**Client responsibility:**

- Update stored JWT and treat the re-issued token's `exp` and `gexp` claims as the canonical source of truth.
- Update stored `expiresAt` and `gracePeriodEndsAt` from the response; these MUST match the new JWT claims. If a discrepancy is detected, JWT claims win.
- Retry automatically on network error before showing a degraded UI.
- Call `/api/refresh` proactively when `now > exp − 86400` (within 24 h of expiry).
- Call `/api/refresh` automatically on reconnect when token is `in_grace` (BF-06).

---

### `POST /api/updates/check`

**Request body (canonical — BF-02):**

```json
{
  "token": "string",
  "currentVersion": "x.y.z"
}
```

- `token`: a valid JWT (must not be expired; grace-period tokens are rejected here).
- `currentVersion`: strict semver (three dot-separated non-negative integers, no pre-release suffix, no leading zeros).

**Server behavior:**

- Rejects malformed JSON, non-object JSON, missing fields, empty fields, or non-strict-semver `currentVersion` values with `400 INVALID_REQUEST`.
- Verifies the JWT with the configured public key and `JWT_KID`.
- Rejects malformed, expired (including grace-period), tampered, or wrong-key JWTs with `401 INVALID_TOKEN`. (Grace-period tokens are not accepted here; only `now ≤ exp` tokens pass.)
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
- Returns `{ "allowed": false }` when `currentVersion` is already at or above the configured latest version.

**Success response — update available (canonical — BF-02):**

```json
{
  "allowed": true,
  "latestVersion": "x.y.z",
  "manifestUrl": "https://example.com/manifest",
  "rollbackVersion": "x.y.z"
}
```

**Success response — no update available (canonical — BF-02):**

```json
{
  "allowed": false
}
```

**Error response:**

```json
{
  "ok": false,
  "error": "INVALID_REQUEST | INVALID_TOKEN | LICENSE_REVOKED | LICENSE_REFUNDED | ACTIVATION_REVOKED | UPDATES_NOT_ENTITLED | CHANNEL_NOT_ALLOWED | SERVER_ERROR"
}
```

**Client responsibility:**

- Parse the response strictly against the canonical shapes above. Any field not listed here MUST be ignored.
- Do NOT call this endpoint with a grace-period or expired token; call `/api/refresh` first.
- Only surface an update prompt when `allowed === true`.

---

### `POST /api/gumroad/webhook`

**Request requirements:**

- `Content-Type` MUST be `application/x-www-form-urlencoded`.
- The body MUST parse as a non-empty form payload.
- `seller_id` MUST match `GUMROAD_SELLER_ID` using constant-time comparison.
- Test events are accepted and ignored.

**Normalized payload fields:**

`sellerId`, `saleId`, `orderNumber`, `productId`, `licenseKey`, `email`, `subscriptionId`, `purchaseIds`, `resourceName`, `cancelled`, `ended`, `restarted`, `recurringCharge`, `cancelledAt`, `endedAt`, `restartedAt`, `endedReason`, `subscriptionEndedAt`, `subscriptionCancelledAt`, `subscriptionFailedAt`, `chargeOccurrenceCount`, `refunded`, `disputed`, `disputeWon`, `saleTimestamp`, `isTest`, `discoverFeeCharged`, `canContact`, `raw`

**Event routing:**

- `purchase`
  - Requires `sale_id`, `order_number`, `product_id`, `license_key`, `email`, and `sale_timestamp`.
  - Unknown product IDs are ignored; respond `200 { "ok": true }`.
  - Known tier products create or update `licenses` and `update_entitlements`, then persist `gumroad_purchase_id`.
- `refund`
  - Marks the license referenced by `gumroad_purchase_id` as refunded.
- `dispute`
  - Marks the license referenced by `gumroad_purchase_id` as refunded.
- `dispute_won`
  - Clears the refunded flag for the matching license.
- `subscription_purchase`, `subscription_cancel`, `subscription_renewal`, `subscription_restart`, `subscription_expired`
  - Route through the subscription entitlement workflow.
  - Unknown subscription product IDs are ignored; respond `200 { "ok": true }`.
  - The workflow is idempotent and replay-aware using `subscription_entitlements.metadata`.
- Unsupported event types are ignored and return `200 { "ok": true }`.

**Success response:**

```json
{ "ok": true }
```

**Error response:**

```json
{
  "error": "INVALID_REQUEST | UNAUTHORIZED_WEBHOOK | SERVER_ERROR"
}
```

**Client responsibility:** none — this endpoint is called by Gumroad only.

---

## `ProductionJwtClaims`

All JWTs issued by the server use exactly these claims. Both sides MUST treat this table as canonical (BF-02).

| Field     | Type                                       | Meaning                                                                      |
| :-------- | :----------------------------------------- | :--------------------------------------------------------------------------- |
| `sub`     | `string`                                   | Internal Supabase license id                                                 |
| `tier`    | `"community" \| "pro"`                     | License tier                                                                 |
| `mid`     | `string`                                   | Machine id bound to the token                                                |
| `channel` | `"stable" \| "beta" \| "nightly" \| "oss"` | Build channel                                                                |
| `flags`   | `integer`                                  | Feature flag bitmask; currently always `0`                                   |
| `iat`     | `integer`                                  | Issued-at time in Unix seconds                                               |
| `exp`     | `integer`                                  | Expiry time in Unix seconds; 30 days after `iat`                             |
| `gexp`    | `integer`                                  | Grace expiry time in Unix seconds; 15 days after `exp` (45 days after `iat`) |
| `uexp`    | `integer`                                  | Update entitlement expiry in Unix seconds; fallback is `exp` when absent      |

**Validation rules (both sides):**

- `sub`, `tier`, `mid`, `channel`, `flags`, `iat`, `exp`, `gexp`, and `uexp` are all required.
- All timestamps MUST be non-negative integers.
- `tier` MUST be `"community"` or `"pro"`.
- `channel` MUST be `"stable"`, `"beta"`, `"nightly"`, or `"oss"`.
- `gexp` MUST be strictly greater than `exp`.
- `exp` MUST be strictly greater than `iat`.
- JWT verification enforces `alg = EdDSA` and the configured `kid`.
- Clock-skew tolerance of +/-300 s applies to `iat` evaluation on both sides (BF-04).

---

## Supabase Table Shapes

### `licenses`

| Column                          | Type        | Notes                                       |
| :------------------------------ | :---------- | :------------------------------------------ |
| `id`                            | `uuid`      | Primary key                                 |
| `gumroad_license_key`           | `text`      | Unique                                      |
| `gumroad_purchase_id`           | `text`      | Nullable                                    |
| `email`                         | `text`      | Nullable                                    |
| `tier`                          | `text`      | `"community"` or `"pro"`                    |
| `activation_limit`              | `integer`   |                                             |
| `is_refunded`                   | `boolean`   |                                             |
| `is_revoked`                    | `boolean`   |                                             |
| `update_entitlement_expires_at` | `timestamp` | Nullable; copied to JWT `uexp` when present |
| `created_at`                    | `timestamp` |                                             |
| `updated_at`                    | `timestamp` |                                             |

### `activations`

| Column            | Type        | Notes                                                                                                                                                                                                    |
| :---------------- | :---------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`              | `uuid`      | Primary key                                                                                                                                                                                              |
| `license_id`      | `uuid`      | FK → `licenses.id`                                                                                                                                                                                       |
| `machine_id`      | `text`      |                                                                                                                                                                                                          |
| `app_version`     | `text`      |                                                                                                                                                                                                          |
| `build_channel`   | `text`      |                                                                                                                                                                                                          |
| `last_active_at`  | `timestamp` |                                                                                                                                                                                                          |
| `last_refresh_at` | `timestamp` |                                                                                                                                                                                                          |
| `is_revoked`      | `boolean`   |                                                                                                                                                                                                          |
| `grace_state`     | `text`      | `"active"`, `"in_grace"`, or `"expired"`. Best-effort mirror of JWT lifecycle state; JWT claims are authoritative. See [§ grace_state Lifecycle](#grace_state-lifecycle) for mandatory transition rules. |
| `created_at`      | `timestamp` |                                                                                                                                                                                                          |

### `update_entitlements`

| Column                        | Type        | Notes                                                |
| :---------------------------- | :---------- | :--------------------------------------------------- |
| `id`                          | `uuid`      | Primary key                                          |
| `license_id`                  | `uuid`      | FK → `licenses.id`                                   |
| `entitled_until`              | `timestamp` |                                                      |
| `is_perpetual`                | `boolean`   | A perpetual entitlement bypasses the timestamp check |
| `grandfathered_below_version` | `text`      | Nullable                                             |
| `created_at`                  | `timestamp` |                                                      |
| `updated_at`                  | `timestamp` |                                                      |

### `subscription_entitlements`

| Column                    | Type        | Notes                                     |
| :------------------------ | :---------- | :---------------------------------------- |
| `id`                      | `uuid`      | Primary key                               |
| `license_id`              | `uuid`      | FK → `licenses.id`                        |
| `feature_key`             | `text`      |                                           |
| `gumroad_subscription_id` | `text`      | Nullable                                  |
| `gumroad_purchase_id`     | `text`      | Unique, nullable                          |
| `product_id`              | `text`      |                                           |
| `status`                  | `text`      | `"active"`, `"cancelled"`, or `"expired"` |
| `starts_at`               | `timestamp` |                                           |
| `entitled_until`          | `timestamp` | Nullable                                  |
| `cancelled_at`            | `timestamp` | Nullable                                  |
| `metadata`                | `jsonb`     | Default `{}`                              |
| `created_at`              | `timestamp` |                                           |
| `updated_at`              | `timestamp` |                                           |

**Constraints:** `(license_id, feature_key)` is unique; `gumroad_purchase_id` is unique.

**Metadata keys used by the subscription workflow:** `feature_key`, `billing`, `product_id`, `subscription_id`, `purchase_id`, `last_event_type`, `last_event_timestamp`, `last_charge_occurrence_count`.

---

## Environment Variables

### Core runtime

| Variable                    | Required | Notes                        |
| :-------------------------- | :------- | :--------------------------- |
| `SUPABASE_URL`              | Yes      | Database access              |
| `SUPABASE_SERVICE_ROLE_KEY` | Yes      | Database access              |
| `JWT_KID`                   | Yes      | JWT key id                   |
| `JWT_PRIVATE_KEY`           | Yes      | Inline PKCS8 private key PEM |
| `JWT_PUBLIC_KEY`            | Yes      | Inline SPKI public key PEM   |

### Gumroad webhook authentication

| Variable            | Required |
| :------------------ | :------- |
| `GUMROAD_SELLER_ID` | Yes      |

### Gumroad tier products

`GUMROAD_PRODUCT_COMMUNITY`, `GUMROAD_PRODUCT_PRO`, `GUMROAD_PRODUCT_IDS`, `GUMROAD_DEFAULT_ACTIVATION_LIMIT`, `GUMROAD_COMMUNITY_ACTIVATION_LIMIT`, `GUMROAD_PRO_ACTIVATION_LIMIT`, `GUMROAD_COMMUNITY_UPDATE_ENTITLEMENT_MODE`, `GUMROAD_PRO_UPDATE_ENTITLEMENT_MODE`, `GUMROAD_COMMUNITY_UPDATE_ENTITLEMENT_DAYS`, `GUMROAD_PRO_UPDATE_ENTITLEMENT_DAYS`, `GUMROAD_COMMUNITY_GRANDFATHERED_BELOW_VERSION`, `GUMROAD_PRO_GRANDFATHERED_BELOW_VERSION`

### Gumroad subscription products

`GUMROAD_SUB_LUT_PACKS_PRODUCT`, `GUMROAD_SUB_CLOUD_BACKUP_PRODUCT`, `GUMROAD_SUB_AI_PROCESSING_PRODUCT`, `GUMROAD_SUB_CREATOR_ASSETS_PRODUCT`, `GUMROAD_SUB_PREMIUM_TEMPLATES_PRODUCT`, `GUMROAD_SUB_PRODUCT_IDS`, `GUMROAD_SUB_LUT_PACKS_BILLING`, `GUMROAD_SUB_CLOUD_BACKUP_BILLING`, `GUMROAD_SUB_AI_PROCESSING_BILLING`, `GUMROAD_SUB_CREATOR_ASSETS_BILLING`, `GUMROAD_SUB_PREMIUM_TEMPLATES_BILLING`

### Update check release targets

| Variable                           | Notes                                    |
| :--------------------------------- | :--------------------------------------- |
| `UPDATES_STABLE_LATEST_VERSION`    |                                          |
| `UPDATES_STABLE_MANIFEST_URL`      | Must be HTTPS and match a trusted origin |
| `UPDATES_STABLE_ROLLBACK_VERSION`  |                                          |
| `UPDATES_BETA_LATEST_VERSION`      |                                          |
| `UPDATES_BETA_MANIFEST_URL`        | Must be HTTPS and match a trusted origin |
| `UPDATES_BETA_ROLLBACK_VERSION`    |                                          |
| `UPDATES_NIGHTLY_LATEST_VERSION`   |                                          |
| `UPDATES_NIGHTLY_MANIFEST_URL`     | Must be HTTPS and match a trusted origin |
| `UPDATES_NIGHTLY_ROLLBACK_VERSION` |                                          |
| `UPDATES_TRUSTED_MANIFEST_ORIGINS` | Comma-separated HTTPS origins            |

Notes:

- The update-check route MUST reject manifest URLs that are not HTTPS or that do not match a trusted origin.
- `oss` is not a valid release target and is always rejected by `/api/updates/check`.
