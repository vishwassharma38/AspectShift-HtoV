# Phase E5 Update UX + Download/Install/Restart Flow

## Summary

Phase E5 adds the production-facing update experience after the existing entitlement gate approves an update.

The update path now works like this:

1. The user requests an update check from the app UI.
2. The desktop app calls `/api/updates/check` first.
3. If entitlement denies the update, the flow stops immediately.
4. If entitlement allows the update, the app calls `tauri-plugin-updater.check()`.
5. If the updater reports a signed update, the user can manually download and install it.
6. After install, the app prompts for restart instead of forcing a silent relaunch.

## Entitlement-First Rule

This phase preserves the existing security boundary:

- `/api/updates/check` remains the first gate.
- `tauri-plugin-updater` is only used after the entitlement response says the license is update-eligible.
- Update failures do not affect runtime license state.
- Update failures do not invoke activation or refresh flows.
- Update failures do not modify the licensing backend contract.

## Final E5 Update Lifecycle

### 1. Idle

- No update dialog is open.
- No update flow is active.
- The app behaves normally.

### 2. Checking entitlement

- The UI shows that the app is verifying update entitlement.
- The desktop sends the current JWT to `/api/updates/check`.
- If the entitlement check fails, the updater is not called.

### 3. Entitlement denied

- The UI reports that the license is not entitled, the release channel is not allowed, or the user must sign in again.
- The flow stops.

### 4. Already latest

- The app shows that the installed build is already current.
- The updater plugin is not asked to download anything.

### 5. Checking updater

- The app calls `tauri-plugin-updater.check()` only after entitlement approval.
- The signed manifest is verified by Tauri updater.
- Release metadata such as the version and release notes can be surfaced in the UI.

### 6. Update available

- The modal shows the current version, the latest version, and release notes when present.
- The user chooses whether to start the install.
- Nothing installs automatically.

### 7. Downloading

- The app calls `update.download()` and tracks download progress events.
- The UI disables duplicate clicks while the download is running.

### 8. Installing

- The app calls `update.install()` after download completes.
- The install step is shown separately from the download step.
- Tauri continues to verify signed updater artifacts.

### 9. Restart required

- After a successful install, the UI shows a restart-required state.
- The user explicitly chooses when to relaunch the app.
- The app uses `@tauri-apps/plugin-process` `relaunch()` for the restart action.

### 10. Failed

- Entitlement failures, manifest failures, download failures, install failures, and restart failures all land in a visible failed state.
- The app logs a structured updater diagnostic message.
- The runtime auth state is not changed.

## Permissions Added

The capability file now grants only the minimum permissions needed for E5:

- `updater:allow-check`
- `updater:allow-download`
- `updater:allow-install`
- `process:allow-restart`

No broader updater or process permissions were added.

## Manual Test Plan

### Level 1: Current `0.1.1` validation

Use this path with the current release state:

- server latest = `0.1.1`
- installed app version = `0.1.1`
- click `Check for Updates`
- the app shows `Already on the latest version`
- `tauri-plugin-updater.check()` is not called
- the license state remains unchanged

### Level 2: Future `0.1.1` validation

Use this path once a signed `0.1.1` release exists:

- publish signed `0.1.1` updater artifacts
- set server latest = `0.1.1`
- installed app version = `0.1.1`
- click `Check for Updates`
- `/api/updates/check` returns `update_available`
- `tauri-plugin-updater.check()` detects the update
- the modal shows the update as available
- click `Download and Install`
- the UI shows download progress
- the UI shows an installing state
- after install succeeds, the app shows `Restart required`
- click `Restart Now`
- the app relaunches into `0.1.1`
- the license and auth state remain intact

## What Remains for E6

Phase E6 should focus on failure recovery and rollback metadata:

- rollback metadata capture
- rollback-aware UI
- install failure remediation
- deeper diagnostics for updater manifest mismatches
- any recovery UX needed after a failed install

## Validation

The following checks should be run for this phase:

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml` if practical

## Notes

- The update UX is intentionally user-controlled.
- No silent startup install path was added.
- The backend licensing contract was not changed.
- Signed updater verification remains handled by Tauri updater.
