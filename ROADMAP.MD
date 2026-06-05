# AspectShift-HtoV Licensing Roadmap

---

## Project Overview

The AspectShift-HtoV licensing ecosystem is a dual-repository system consisting of a Tauri/Rust desktop application and a Next.js edge licensing backend. The system implements offline-first license validation through cryptographically signed JWTs, backed by a globally distributed Vercel Edge Runtime and Supabase as the persistent licensing datastore.

The architecture separates three distinct concerns:

- **Runtime feature access** — controlled by local JWT verification on the desktop
- **Update entitlement** — controlled by edge verification against purchase and maintenance records
- **OSS/self-built usage** — permitted without restriction; updater disabled by default

The desktop application operates without network dependency at startup by loading and verifying a locally persisted JWT. Background refresh and grace-state handling insulate the user from transient network failures.

---

## Repository Ownership

### Ownership Matrix

| Responsibility                                   | AspectShift-HtoV (Desktop) | AspectShift-HtoV-License-Server (Server) |
| ------------------------------------------------ | -------------------------- | ---------------------------------------- |
| AuthManager lifecycle                            | ✅ Owner                   | —                                        |
| Local JWT verification                           | ✅ Owner                   | —                                        |
| `auth://state` persistence                       | ✅ Owner                   | —                                        |
| Grace-state lifecycle                            | ✅ Owner                   | —                                        |
| Update entitlement cache                         | ✅ Owner                   | —                                        |
| Updater integration                              | ✅ Owner                   | —                                        |
| OSS/commercial build behavior                    | ✅ Owner                   | —                                        |
| Offline-first execution                          | ✅ Owner                   | —                                        |
| License validation logic                         | —                          | ✅ Owner                                 |
| JWT issuance                                     | —                          | ✅ Owner                                 |
| Activation lifecycle (`/activate`)               | —                          | ✅ Owner                                 |
| Refresh lifecycle (`/refresh`)                   | —                          | ✅ Owner                                 |
| Update entitlement evaluation (`/updates/check`) | —                          | ✅ Owner                                 |
| Gumroad webhook processing                       | —                          | ✅ Owner                                 |
| Purchase lifecycle management                    | —                          | ✅ Owner                                 |
| Supabase schema & data layer                     | —                          | ✅ Owner                                 |
| `/activate` request/response contract            | ✅ Shared                  | ✅ Shared                                |
| `/refresh` request/response contract             | ✅ Shared                  | ✅ Shared                                |
| Update entitlement integration                   | ✅ Shared                  | ✅ Shared                                |
| JWT payload schema                               | ✅ Consumer                | ✅ Producer                              |

---

## Current Progress Summary

### Completed ✅

- D6 — Gumroad webhook infrastructure (server-side, all subphases)
- D7.1 — AuthManager Foundation
- D7.2 — Persistence Hardening (`auth://state`)
- D7.3 — Local JWT Verification Hardening
- D7.4 — `/activate` Integration
- D7.5 — Silent `/refresh` Integration
- D7.6 — Grace-State Lifecycle

### In Progress 🚧

- D7.7 — Update Entitlement Integration
- D7.8 — OSS vs Commercial Behavior
- D7.9 — End-to-End Desktop Validation

### Planned ⬜

- Phase E — Paid Updates & Edge-Case Architecture (all subphases)

---

## Phase D — Real Licensing Backend

### Phase Objective

Replace the temporary/local simulator architecture with a globally distributed, production-grade edge licensing infrastructure. Phase D establishes the single source of truth for all licensing operations, including license validation, JWT issuance, offline-first execution, and clean separation of runtime access, update entitlement, and OSS usage.

---

### D1 — Edge Infrastructure

**Purpose**
Establish the backend runtime foundation on Vercel Edge Functions using V8 isolates.

**Architecture Summary**
All licensing endpoints execute on the global Vercel Edge Runtime. Endpoints are stateless edge handlers. Benefits: near-zero cold starts, globally distributed verification, no traditional server management, horizontal scalability by default.

**Repository Ownership**
Server only.

**Current Status**
✅ Completed

**Completion Notes**
Edge Runtime infrastructure is live. All licensing endpoints operate as stateless handlers on Vercel.

---

### D2 — Supabase Data Layer

**Purpose**
Establish the persistent licensing datastore.

**Architecture Summary**
Supabase Postgres accessed via edge-compatible `@supabase/supabase-js` over HTTPS Data API (no persistent DB connection pooling). Three primary tables:

| Table                 | Stores                                                                                                                                       |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `licenses`            | Gumroad purchase ID, license key, email, activation limits, subscription/refund status, update entitlement expiry, purchase tier, timestamps |
| `activations`         | Machine fingerprints, activation timestamps, last refresh timestamps, grace state history, revoked device states                             |
| `update_entitlements` | Eligible update window, perpetual license cutoff, maintenance expiration, grandfathered versions, lifetime upgrade flags                     |

**Repository Ownership**
Server only.

**Current Status**
✅ Completed

**Completion Notes**
Schema deployed. All three tables operational. Update entitlement table intentionally separates update eligibility from runtime activation.

---

### D3 — Edge Runtime Endpoints

**Purpose**
Implement the three core licensing API endpoints.

**Architecture Summary**

`/activate`

- Inputs: Gumroad license key, machine fingerprint, app version, optional purchase token
- Flow: validate key → check activation limits → verify refund/revocation state → register activation → generate signed JWT → store to `auth://state`
- Response: signed JWT, refresh expiry, update entitlement metadata, grace policy metadata

`/refresh`

- Inputs: existing JWT + machine fingerprint
- Flow: verify JWT signature → re-check Supabase entitlement → detect refunds/cancellations/revocations → renew JWT expiry → update activation heartbeat
- Response: refreshed JWT with updated claims

`/updates/check`

- Purpose: dedicated updater entitlement endpoint, separated from runtime feature access
- Validates: JWT validity, update entitlement expiry, build channel, rollout eligibility, revoked purchase states
- Response: allowed update version, signed update manifest, rollout metadata, rollback metadata

**Repository Ownership**
Server only (endpoint implementation). Desktop owns request initiation and response consumption.

**Current Status**
✅ Completed

**Completion Notes**
All three endpoints are operational. `/activate` and `/refresh` contracts are shared between repositories.

---

### D4 — Cryptography Layer

**Purpose**
Implement edge-compatible cryptographic signing and verification for all JWTs.

**Architecture Summary**
Node.js native crypto is unavailable in Edge Runtime. JWT signing and verification use Web Crypto API and `jose`. All JWTs are signed and tamper-detectable. Local offline verification is supported without network dependency.

**JWT Payload Schema**

| Claim                     | Description                   |
| ------------------------- | ----------------------------- |
| Machine fingerprint hash  | Binds token to device         |
| License ID                | Links to Supabase record      |
| Issue timestamp           | Token origin                  |
| Expiry timestamp          | Token validity window         |
| Grace expiry              | Grace-state boundary          |
| Feature flags             | Runtime capability gates      |
| Purchase tier             | Entitlement level             |
| Update entitlement expiry | Update access boundary        |
| Build channel             | stable / beta / nightly / OSS |
| Rollback eligibility      | Rollback authorization flag   |

JWTs intentionally exclude sensitive personal data.

**Repository Ownership**
Server only (signing). Desktop is consumer (verification).

**Current Status**
✅ Completed

---

### D5 — Local State Architecture

**Purpose**
Implement encrypted local state storage to enable offline-first execution.

**Architecture Summary**
`auth://state` is the encrypted local store on the desktop. Stores: JWT, refresh timestamps, grace state, update entitlement cache, last successful verification timestamp. This enables instant offline validation with no startup network dependency.

**Repository Ownership**
Desktop only.

**Current Status**
✅ Completed

---

### D6 — Gumroad Webhook Infrastructure

**Purpose**
Implement always-on webhook receiver for the complete Gumroad purchase lifecycle.

**Architecture Summary**
Vercel webhook receiver handles all Gumroad purchase events. On successful purchase: verify webhook authenticity, create/update license record, generate purchase token, create update entitlement entry, store purchase metadata. Also handles: refunds, disputes, subscription cancellations, maintenance renewals.

**Repository Ownership**
Server only.

**Current Status**
✅ Completed — all D6 subphases complete.

**Completion Notes**
See D6 subphase table in the Phase D Subphase Tracking section below.

---

### D7 — Desktop AuthManager Integration

**Purpose**
Integrate the desktop application with the live licensing backend through a complete AuthManager implementation.

**Architecture Summary**
AuthManager mirrors the existing DepsManager lifecycle pattern for internal consistency. Covers: JWT load and local verification on startup, `/activate` integration, silent `/refresh` integration, grace-state lifecycle, update entitlement cache, and OSS vs commercial build behavioral separation.

**Repository Ownership**
Desktop primary. Server shared for `/activate` and `/refresh` contract.

**Current Status**
🚧 In Progress — D7.1–D7.6 complete; D7.7–D7.9 pending.

---

## Phase D Subphase Tracking

| Subphase | Description                          | Owner   | Status       |
| -------- | ------------------------------------ | ------- | ------------ |
| D6.1     | Webhook contract                     | Server  | ✅ Completed |
| D6.2     | Signature verification               | Server  | ✅ Completed |
| D6.3     | Purchase handler                     | Server  | ✅ Completed |
| D6.4     | Refund/dispute handler               | Server  | ✅ Completed |
| D6.5     | Subscription cancellation handler    | Server  | ✅ Completed |
| D6.6     | Maintenance renewal handler          | Server  | ✅ Completed |
| D6.7     | Idempotency hardening                | Server  | ✅ Completed |
| D6.8     | Replay protection                    | Server  | ✅ Completed |
| D6.9     | Validation matrix                    | Server  | ✅ Completed |
| D7.1     | AuthManager foundation               | Desktop | ✅ Completed |
| D7.2     | `auth://state` persistence hardening | Desktop | ✅ Completed |
| D7.3     | Local JWT verification hardening     | Desktop | ✅ Completed |
| D7.4     | `/activate` integration              | Both    | ✅ Completed |
| D7.5     | Silent `/refresh` integration        | Both    | ✅ Completed |
| D7.6     | Grace-state lifecycle                | Server  | ✅ Completed |
| D7.7     | Update entitlement integration       | Both    | ⬜ Planned   |
| D7.8     | OSS vs commercial behavior           | Desktop | ⬜ Planned   |
| D7.9     | End-to-end desktop validation matrix | Desktop | ⬜ Planned   |

---

## D7 Subphase Ownership Detail

| Subphase                            | License Server | Tauri App | Both  |
| ----------------------------------- | -------------- | --------- | ----- |
| D6.1 Webhook contract               | ✅             | —         | —     |
| D6.2 Signature verification         | ✅             | —         | —     |
| D6.3 Purchase handler               | ✅             | —         | —     |
| D6.4 Refund/dispute handler         | ✅             | —         | —     |
| D6.5 Subscription cancellation      | ✅             | —         | —     |
| D6.6 Maintenance renewal            | ✅             | —         | —     |
| D6.7 Idempotency hardening          | ✅             | —         | —     |
| D6.8 Replay protection              | ✅             | —         | —     |
| D6.9 Validation matrix              | ✅             | —         | —     |
| D7.1 AuthManager foundation         | —              | ✅        | —     |
| D7.2 `auth://` persistence          | —              | ✅        | —     |
| D7.3 Local JWT verification         | —              | ✅        | —     |
| D7.4 `/activate` integration        | ✅             | ✅        | ✅    |
| D7.5 Silent `/refresh` integration  | ✅             | ✅        | ✅    |
| D7.6 Grace-state lifecycle          | Minor          | ✅        | Minor |
| D7.7 Update entitlement integration | ✅             | ✅        | ✅    |
| D7.8 OSS vs commercial behavior     | Minor          | ✅        | Minor |
| D7.9 Desktop validation matrix      | —              | ✅        | Minor |

---

## Phase E — Paid Updates & Edge-Case Architecture

### Phase Objective

Deliver a seamless, professional offline-first licensing experience where auth never blocks UI, update entitlement is enforced globally, transient outages never interrupt workflow, and production updates are delivered securely and bandwidth-efficiently.

---

### E1 — Update Entitlement Enforcement

**Purpose**
Gate the official auto-updater behind valid JWT, valid purchase token, and non-expired update entitlement.

**Architecture Summary**
Update requests include `Authorization: Bearer <JWT>`. The `/updates/check` edge endpoint validates entitlement before any binary delivery occurs. Invalid requests never consume storage bandwidth. Runtime access and update access are independently tracked:

| State               | Runtime Access | Updates     |
| ------------------- | -------------- | ----------- |
| Purchased (active)  | ✅             | ✅          |
| Maintenance expired | ✅             | ❌          |
| Refunded            | ✅             | ❌          |
| Self-built OSS      | ✅             | Manual only |

**Repository Ownership**
Shared — Server owns entitlement evaluation; Desktop owns updater request construction and token attachment.

**Dependencies**
D7.7 (update entitlement integration), D3 `/updates/check` endpoint, valid JWT in `auth://state`.

**Status**
⬜ Planned

---

### E2 — Updater Integration

**Purpose**
Integrate `tauri-plugin-updater` with the edge-gated update manifest delivery system.

**Architecture Summary**
Zero-delay optimistic boot flow: AuthManager loads `auth://state`, verifies JWT locally, unlocks the dashboard immediately, then dispatches background refresh asynchronously. No blocking spinners for healthy tokens. Update checks occur after UI is interactive.

Background refresh lifecycle: async thread dispatches `/refresh`, updated JWT is persisted atomically, UI remains fully responsive throughout.

Trust chain:

```
GitHub Actions
    ↓
Signed binaries
    ↓
Signed update manifest
    ↓
Edge entitlement verification
    ↓
tauri-plugin-updater
```

**Repository Ownership**
Desktop primary. Server provides signed manifest endpoint.

**Dependencies**
E1 (update entitlement enforcement), signed binary pipeline (E4), `tauri-plugin-updater` configuration.

**Status**
⬜ Planned

---

### E3 — Rollback Infrastructure

**Purpose**
Implement safe rollback capability to protect users from failed or corrupted updates.

**Architecture Summary**
If update verification fails: revert to previous signed binary, preserve local `auth://state`, mark failed release, prevent repeated installation of a bad release. Rollback metadata is distributed through the edge update manifest.

**Repository Ownership**
Shared — Server produces rollback manifest entries; Desktop consumes and executes rollback logic via `tauri-plugin-updater`.

**Dependencies**
E2 (updater integration), E4 (release pipeline generating rollback manifests).

**Status**
⬜ Planned

---

### E4 — Release Pipeline

**Purpose**
Automate binary building, signing, manifest generation, and release publication via GitHub Actions.

**Architecture Summary**
GitHub Actions pipeline: build binaries → sign artifacts → generate update manifests → upload releases → publish rollout metadata. Supports separate release channels: `stable`, `beta`, `nightly`, `OSS`. Commercial builds and OSS builds use distinct channels.

Every production binary must be signed. OSS channel publishes to GitHub Releases without entitlement gating.

**Repository Ownership**
Desktop repository owns the GitHub Actions pipeline and binary artifacts. Server owns the update manifest hosting endpoint.

**Dependencies**
Code-signing certificates, Vercel deployment for manifest endpoint, Tauri build configuration per channel.

**Status**
⬜ Planned

---

### E5 — Telemetry

**Purpose**
Implement privacy-safe, non-blocking crash and diagnostic reporting for production commercial builds.

**Architecture Summary**
Production builds include: crash logging, structured diagnostics, update failure telemetry, activation error reporting. Requirements: privacy-safe, opt-in where legally required, non-blocking async reporting. Telemetry must never block app startup. OSS builds exclude production telemetry by default.

**Repository Ownership**
Desktop primary. Server may provide a telemetry ingestion endpoint.

**Dependencies**
OSS vs commercial build flag (D7.8), E4 release pipeline (channel identification).

**Status**
⬜ Planned

---

### E6 — Commercial Distribution

**Purpose**
Finalize commercial distribution artifacts and ensure OSS/commercial build separation is clean and auditable.

**Architecture Summary**

**Official Commercial Builds** (distributed through Gumroad):

- Signed installers
- Auto-updates via `tauri-plugin-updater`
- Updater entitlement verification
- Production telemetry
- Rollback manifests
- Require purchase token and valid JWT

**Self-Built OSS Builds** (built from GitHub source):

- Fully functional runtime
- No forced feature lock
- Updater disabled by default
- Manual updates via GitHub Releases
- Optional "support development" banner
- GPL/open-source compliant

This separation keeps GPL compliance clean and does not restrict OSS self-builders from runtime functionality.

**Repository Ownership**
Desktop primary (build configuration, installer signing, OSS flag behavior). Server minor (purchase token validation remains in place).

**Dependencies**
D7.8 (OSS vs commercial behavior), E4 (release pipeline), E1 (update entitlement enforcement), code-signing setup.

**Status**
⬜ Planned

---

## Milestone Timeline

### Completed Milestones

| Milestone                              | Description                                                                 |
| -------------------------------------- | --------------------------------------------------------------------------- |
| Edge Runtime operational               | Vercel Edge Functions live for all licensing endpoints                      |
| Supabase schema deployed               | `licenses`, `activations`, `update_entitlements` tables operational         |
| Gumroad webhook pipeline complete      | Full purchase lifecycle: activation, refund, dispute, cancellation, renewal |
| Webhook security hardened              | Signature verification, idempotency, replay protection                      |
| AuthManager foundation                 | Desktop AuthManager scaffolded and integrated with `auth://state`           |
| Persistence hardening                  | `auth://state` atomic read/write with encryption                            |
| JWT verification hardened              | Offline local JWT verification without network dependency                   |
| `/activate` integration complete       | Desktop ↔ Server activation flow end-to-end                                 |
| Silent `/refresh` integration complete | Background JWT refresh with atomic persistence                              |
| Grace-state lifecycle complete         | 48-hour configurable grace with silent retry and state degradation          |

### Remaining Milestones

| Milestone                        | Description                                                                                 | Phase |
| -------------------------------- | ------------------------------------------------------------------------------------------- | ----- |
| Update entitlement integration   | Desktop reads and caches update entitlement claims from JWT; `/updates/check` integrated    | D7.7  |
| OSS vs commercial build behavior | Runtime flag gates updater, telemetry, and commercial features                              | D7.8  |
| End-to-end desktop validation    | Full validation matrix across activation, refresh, grace, revocation, and offline scenarios | D7.9  |
| Update entitlement enforcement   | Edge-gated updater; entitlement model enforced per purchase state                           | E1    |
| Updater integration              | `tauri-plugin-updater` wired to signed edge manifest with JWT auth                          | E2    |
| Rollback infrastructure          | Rollback manifest generation and desktop rollback execution                                 | E3    |
| Release pipeline                 | GitHub Actions pipeline for signed multi-channel binary releases                            | E4    |
| Telemetry                        | Non-blocking crash/diagnostic reporting for commercial builds                               | E5    |
| Commercial distribution          | Signed installer pipeline, OSS/commercial build separation finalized                        | E6    |

### Path to Production

The following sequence represents the recommended implementation order from the current project state through final release readiness.

```
1. D7.7 — Update Entitlement Integration
   Desktop reads update entitlement claims; integrates with /updates/check endpoint.

2. D7.8 — OSS vs Commercial Behavior
   Build flag separates updater, telemetry, and commercial behavior at compile time.

3. D7.9 — End-to-End Desktop Validation Matrix
   Full scenario coverage: activation, refresh, grace expiry, revocation, offline, fingerprint mismatch.

4. E1 — Update Entitlement Enforcement
   Edge-gated update requests; entitlement model validated against all purchase states.

5. E4 — Release Pipeline
   GitHub Actions pipeline for signed binaries and update manifests; multi-channel support.

6. E2 — Updater Integration
   tauri-plugin-updater wired to edge manifest; optimistic boot flow; JWT attached to update requests.

7. E3 — Rollback Infrastructure
   Rollback manifest entries; desktop rollback execution on failed installs.

8. E5 — Telemetry
   Async, non-blocking crash and update failure reporting for commercial channel.

9. E6 — Commercial Distribution
   Signed installers, Gumroad distribution pipeline, OSS build separation audited and finalized.
```

---

## Final Architecture Outcome

Upon completion of all roadmap items, the AspectShift-HtoV licensing system will provide the following guarantees:

| Capability                    | Description                                                                                              |
| ----------------------------- | -------------------------------------------------------------------------------------------------------- |
| Offline-first licensing       | App launches and operates without network access using locally persisted, cryptographically verified JWT |
| JWT trust chain               | All tokens signed via Web Crypto API / `jose`; tamper-detectable; verifiable offline                     |
| Gumroad automation            | Full purchase lifecycle automated via webhook: activation, refund, dispute, cancellation, renewal        |
| Update entitlement separation | Update access and runtime feature access are independently tracked and enforced                          |
| Commercial distribution       | Signed binaries distributed via Gumroad with auto-update and production telemetry                        |
| OSS coexistence               | Self-built OSS usage permitted without restriction; updater disabled; GPL compliance maintained          |
| Grace-state resiliency        | Configurable grace window protects users from transient network failures without UX disruption           |
| Rollback support              | Rollback manifests distributed via edge; failed updates revert automatically to last signed binary       |
| Signed update delivery        | Every production binary is signed; update manifests are signed and edge-verified before delivery         |
| Architecture symmetry         | AuthManager mirrors DepsManager lifecycle patterns for internal consistency and maintainability          |
