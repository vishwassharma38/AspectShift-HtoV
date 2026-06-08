# AspectShift-HtoV

This repo now includes an updater-signing readiness audit for Phase E1:

- [E1_UPDATER_SIGNING_AUDIT.md](./E1_UPDATER_SIGNING_AUDIT.md)
- [E2_RELEASE_PIPELINE_AUDIT.md](./E2_RELEASE_PIPELINE_AUDIT.md)

Updater signing is separate from the JWT licensing keypair used by the auth backend. The updater private key must stay out of the repository and in CI secrets or secure local storage.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
