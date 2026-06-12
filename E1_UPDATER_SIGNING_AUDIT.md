# Phase E1 Updater Audit + Signing Key Setup

## Summary

E1 is focused on updater foundation readiness, not the full install flow. The repo already has most of the plumbing needed to begin signed update delivery, but it still lacks a release pipeline and a real JWT-authenticated update manifest handoff.

Current state:

- `tauri-plugin-updater` is installed and registered in Rust.
- The frontend already calls the updater `check()` API after a separate license entitlement check.
- The updater configuration already contains an endpoints array and a public key field.
- Update artifact generation was missing from bundle config until this audit.
- Frontend capability permissions were missing for updater `check()`, which would have blocked the JS call at runtime.
- No updater private key is committed to the repo.
- No GitHub Actions release pipeline exists yet.

## Updater Architecture Audit

### 1. Existing updater foundation

- `src-tauri/Cargo.toml` already includes `tauri-plugin-updater = "2"`.
- `src-tauri/src/lib.rs` already registers the plugin with `.plugin(tauri_plugin_updater::Builder::new().build())`.
- `src-tauri/tauri.conf.json` already defines an `updater` block with endpoints, dialog behavior, and a public key.
- `src/App.tsx` already imports `check` from `@tauri-apps/plugin-updater`.
- `src/App.tsx` already gates updater probing behind `check_update_entitlement`.
- The current license/update UI flow in `src/App.tsx` exposes update checks through the reachable app shell and modal flow.

### 2. Gaps found

- `bundle.createUpdaterArtifacts` was not enabled, so release builds were not explicitly configured to emit updater artifacts and signatures.
- `src-tauri/capabilities/default.json` did not grant any updater permission, so the frontend `check()` call was not authorized by the Tauri ACL.
- At the time of the E1 audit, no release workflow existed in `.github/workflows`. The E2 pipeline now fills that gap.
- No release/upload automation existed in the repo before E2.
- The updater flow is still split from the edge entitlement gate: the app checks entitlement first, then probes the updater plugin separately. That is safe as a staging pattern, but it is not yet the final E2 handoff.
- There is no documentation file in the repo that explains the updater key lifecycle, signing requirements, and release prerequisites until now.

### 3. Current security posture

- Updater signature verification is a separate trust chain from JWT licensing.
- JWTs control runtime entitlement and `/api/updates/check` access.
- Tauri updater signatures control artifact authenticity.
- These must remain separate, and this repo now documents that split explicitly.

## Files Inspected

- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `src-tauri/src/lib.rs`
- `src-tauri/src/main.rs`
- `src-tauri/capabilities/default.json`
- `src/App.tsx`
- `src/types/backend.ts`
- `src-tauri/src/auth/auth_commands.rs`
- `src-tauri/src/auth/manager/auth_manager.rs`
- `src-tauri/src/auth/providers/production_provider.rs`
- `src-tauri/src/auth/config/auth_config.rs`
- `src-tauri/src/auth/contracts/CONTRACTS.md`
- `package.json`
- `README.md`
- `ROADMAP.MD`
- `.env`
- `.gitignore`

### Repository areas checked for release automation

- `.github/`
- release/build scripts
- `.env.example`

## Files Modified

- `src-tauri/tauri.conf.json`
- `src-tauri/capabilities/default.json`
- `README.md`
- `E1_UPDATER_SIGNING_AUDIT.md`

## Signing Key Setup Instructions

Use the Tauri updater signing pair, not the JWT licensing keypair.

1. Generate a new updater signing keypair with the Tauri CLI.
1. Store the private key outside the repository.
1. Commit only the public key in `src-tauri/tauri.conf.json`.
1. Keep the private key in a secure local location or CI secret store.
1. Set the signing environment variables only in the build environment.
1. Build release artifacts after `createUpdaterArtifacts` is enabled.

Recommended Tauri CLI flow:

```powershell
npx tauri signer generate -w ~/.tauri/aspectshift-updater.key
```

Build-time environment variables:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY="Path or content of your private key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
```

Important:

- Do not place updater signing secrets in `.env`.
- Do not commit the updater private key.
- Do not reuse the JWT signing keys for updater signing.
- If the private updater key is lost, existing installed users cannot receive new signed updates from that keypair.

## Required Secrets / Env / Config Checklist

### Tauri updater

- `src-tauri/tauri.conf.json`
  - `plugins.updater.endpoints`
  - `plugins.updater.pubkey`
  - `bundle.createUpdaterArtifacts = true`
- `src-tauri/capabilities/default.json`
  - `updater:allow-check`
- Build environment
  - `TAURI_SIGNING_PRIVATE_KEY`
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` when needed

### Licensing backend remains separate

- JWT licensing keys remain server-side only.
- `/api/updates/check` remains the entitlement gate.
- Tauri updater signatures verify release artifacts independently of JWT validation.

## Risks and Pitfalls

- The updater frontend call can still fail if the configured endpoint does not return a valid updater manifest.
- A placeholder public key or placeholder endpoint can make the app look configured while still being unusable for real releases.
- The updater private key must never be stored in the repo or in a normal `.env` file.
- GitHub release automation is still absent, so signed artifacts are not being published by CI yet.
- The current app flow still checks entitlement and updater availability separately; E2 will need to connect those pieces into a real delivery path.

## Post-E1 Readiness Verdict

E1 is ready for signed update delivery foundation work, not for automatic update installs.

What is now true:

- updater plugin is present and registered
- updater artifact generation is enabled
- frontend updater check permission is granted
- public key configuration is present
- private key is not in the repo

What is still missing:

- E2 updater install flow
- JWT-authenticated updater manifest handoff
- release pipeline
- rollback infrastructure
- distribution automation

## What Remains for E2

- Attach the JWT to updater manifest requests
- Consume real edge-hosted update manifests
- Add download/install UX
- Keep install flow behind entitlement and channel checks
- Avoid coupling updater signing with licensing JWT validation
- Prepare for rollback and restart handling

## Validation Notes

The following validation commands should be run after the E1 changes:

- `npm run build`
- `cargo check` from `src-tauri`
- any existing tests relevant to updater or auth contracts

If these pass, the repo is ready for the next updater phase without changing licensing behavior.
