# AspectShift-HtoV

AspectShift-HtoV is a desktop video repurposing and batch conversion app built with Tauri, React, TypeScript, and Rust. It is designed to help transform videos into common aspect ratios and platform-ready exports with local processing, preset-based workflows, and subtitle support.

## Project Overview

- Batch-converts videos into horizontal and vertical delivery formats.
- Uses FFmpeg and FFprobe as bundled sidecars for media processing.
- Provides a preview-driven UI for selecting inputs, outputs, presets, and effects.
- Stores app state locally, including custom presets, settings, runtime cache, and authentication data.

## Features

- Built-in aspect ratio targets: `9:16`, `1:1`, `4:5`, `2:3`, and `16:9`.
- Built-in platform presets: YouTube, YouTube Shorts, Instagram Square, Instagram Reels, TikTok, Twitter/X, and Reddit.
- Batch processing for multiple files and folders.
- Custom presets that can be saved and reused locally.
- Output formats: `MP4`, `MOV`, and `WEBM`.
- Video effects and transforms including rotation, horizontal/vertical flip, blur background, white background, logo overlay, audio removal, and skip-existing output handling.
- Subtitle workflows for generation, export to `.SRT`, and burn-in rendering when the subtitle dependencies are available.
- Queue and log views for tracking batch progress.
- Theme toggle and preview guides in the desktop UI.
- License activation, refresh, and update-entitlement flows for official builds.

## Installation

### Prerequisites

- Node.js
- Rust toolchain
- npm

### Local Development

1. Install dependencies.

   ```bash
   npm install
   ```

2. Start the Tauri development app.

   ```bash
   npm run tauri:dev
   ```

3. Build the frontend and type-check the TypeScript sources.

   ```bash
   npm run build
   ```

## Usage

1. Launch the app.
2. Select one file, a folder, or multiple inputs for batch processing.
3. Choose an output directory.
4. Select one or more aspect ratios or built-in presets.
5. Configure encoding, transform, logo, background, audio, and subtitle options as needed.
6. Start the batch and monitor the queue and log panels.

The UI also supports opening the output folder and saving custom presets for repeat workflows.

## Platform Support

- The app is configured as a Tauri desktop application.
- The Tauri bundle configuration targets all Tauri-supported targets in `src-tauri/tauri.conf.json`.
- The checked-in release workflow currently builds Windows releases.
- The repository includes Windows FFmpeg and FFprobe sidecars under `src-tauri/bin/`.
- Runtime path logic includes platform-specific Whisper binary filenames for Windows, macOS, and Linux.

## Privacy & Data Handling

- Video processing is performed locally.
- The included EULA states that user videos, exported files, subtitles, projects, and local processing data are not uploaded to Software From Vish servers.
- The app may connect to the configured license server for activation, refresh, and update-entitlement checks.
- The app may download update metadata and, when subtitle features are used, Whisper binaries and models from the configured manifest sources.
- License credentials are stored in the operating system keyring.
- App settings, presets, dependency state, logs, exports, and related runtime files are stored locally in the app data/runtime directory tree.
- The EULA states that the app does not intentionally collect analytics or crash reports.
- Users are responsible for keeping backups of original media before processing.

## License

- Source code is licensed under `GPL-3.0-or-later`.
- See [`LICENSE/README.md`](LICENSE/README.md), [`LICENSE/LICENSES/GPL-3.0.txt`](LICENSE/LICENSES/GPL-3.0.txt), and [`LICENSE/THIRD-PARTY-NOTICES.txt`](LICENSE/THIRD-PARTY-NOTICES.txt).
- Official compiled builds, activation, and update access are governed by [`src-tauri/EULA.txt`](src-tauri/EULA.txt).
- The installer EULA does not override GPL rights for the open-source source code.

## Contributing

- Use standard GitHub pull requests for changes.
- Keep modifications aligned with the repository license and bundled notices.
- Run `npm run build` before opening a PR if your changes affect the frontend or shared types.

## Acknowledgements

- FFmpeg and FFprobe
- whisper.cpp
- OpenAI Whisper
- Tauri, React, Vite, and the other third-party dependencies listed in the repository notices
- See [`LICENSE/THIRD-PARTY-NOTICES.txt`](LICENSE/THIRD-PARTY-NOTICES.txt) for the full attribution list
