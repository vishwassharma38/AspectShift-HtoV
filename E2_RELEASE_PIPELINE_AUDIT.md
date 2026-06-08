# Phase E2 Release Pipeline Audit

## Summary

E2 adds the release pipeline that turns the prepared updater foundation into signed GitHub Release assets on Windows.

What this phase does:

- builds the app in production mode
- signs updater bundles with Tauri updater signing secrets
- creates or updates GitHub Releases
- uploads installer and updater assets
- preserves updater JSON metadata for later manifest delivery

What this phase does not do:

- no updater install/download UI
- no runtime auto-update handoff
- no rollback
- no JWT attachment to updater requests
- no licensing changes

## Architecture Audit

### Existing build commands

- `npm run build` runs TypeScript compilation and Vite production bundling.
- `npm run tauri:dev` runs Tauri in dev-auth mode for local development.
- `npm run tauri` is available for release builds and is the command the release workflow uses through Tauri Action.
- `src-tauri/tauri.conf.json` already points `beforeBuildCommand` at `npm run build`, so Tauri release builds will build the frontend automatically.

### Existing Tauri build command

- `tauri-plugin-updater` is installed in `[src-tauri/Cargo.toml](./src-tauri/Cargo.toml)`.
- The plugin is registered in `[src-tauri/src/lib.rs](./src-tauri/src/lib.rs)`.
- Updater artifacts are enabled in `[src-tauri/tauri.conf.json](./src-tauri/tauri.conf.json)`.
- The updater public key is already embedded in `[src-tauri/tauri.conf.json](./src-tauri/tauri.conf.json)`.

### Workflow and release surface

- `.github/workflows` did not exist before this phase.
- No release workflow was present before this phase.
- No release scripts were present in the repo.

### Platform and setup expectations

- The release workflow is Windows-only for now.
- Node.js is not pinned in the repo; the workflow uses the current LTS line.
- Rust is pinned to stable in the workflow.
- The Windows runner is `windows-latest`.

### Signing and secrets posture

- Updater artifact signing is enabled.
- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are documented as build-time secrets.
- The updater private key is not committed anywhere in the repo.
- JWT licensing keys remain unrelated to the updater signing system.

## Files Inspected

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `src-tauri/capabilities/default.json`
- `README.md`
- `E1_UPDATER_SIGNING_AUDIT.md`
- `.gitignore`
- `.env`
- `.github/`
- release/build scripts

## Files Modified

- `.github/workflows/release.yml`
- `E2_RELEASE_PIPELINE_AUDIT.md`

## GitHub Secrets Required

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` only when the key is password-protected

Do not use license-server secrets in this workflow.

## Release Procedure

1. Update the release version manually if the app version and tag need to stay aligned.
1. Create and push a tag like `v1.0.0`, or use the manual workflow with a `release_tag` input.
1. Let GitHub Actions run the Windows release job.
1. The workflow checks out the repo, installs Node dependencies, sets up Rust stable, and runs Tauri release packaging.
1. Tauri Action creates the GitHub Release and uploads the Windows installer, updater bundle, updater signatures, and updater JSON.

## Artifact Expectations

Expected release outputs include:

- Windows installer bundle
- updater bundle artifact
- updater signature files
- updater JSON metadata for GitHub Release delivery

Exact filenames can vary by Tauri version and bundle type, so the workflow relies on Tauri Action's release upload handling instead of hardcoded target globs.

## Verification

To verify the release artifacts after a run:

- confirm the GitHub Release contains the installer asset
- confirm the release contains the updater bundle and `.sig` files
- confirm the release contains the updater JSON output
- compare the release assets to the version tag
- verify the updater signature using the Tauri updater tooling or by testing the release in a separate signed install environment

## Remaining Work for E3

- host or route the manifest delivery path that the updater will consume
- connect the release JSON to the runtime update flow
- add rollback metadata and rollback execution
- decide how later phases will handle JWT-authenticated update requests

## Risks and Pitfalls

- The workflow assumes the release tag and app version stay aligned manually.
- The workflow does not automate version bumping.
- Missing GitHub Secrets will cause the release job to fail, but local builds remain unaffected.
- The release pipeline does not change the licensing flow.
- The workflow only targets Windows for now, so multi-OS release artifacts are intentionally deferred.

## Validation Notes

Local validation before this phase completed successfully:

- `npm run build`
- `cargo check` from `src-tauri`
- `cargo test` from `src-tauri`

The new workflow itself is YAML-only and does not affect local builds.
