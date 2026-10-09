# Platform Export Preset Research and Implementation Report

**Reviewed:** 2026-10-09  
**Scope:** The seven built-in presets in `src-tauri/resources/presets/platform_specific_presets.json`, plus the existing Rust-to-FFmpeg export path.  
**Status:** Implemented on review branch `platform-export-presets-2026-10`; this is not merged into `experimental` or `main`.

## Summary

The presets now use an x264 `fast` speed preset instead of `slow`, have consistent quality labels for their selected CRF values, and explicitly use platform-sized frames. Platform exports with audio request 48 kHz audio; H.264 platform exports explicitly select High Profile; the existing `yuv420p` output and MP4 `+faststart` behavior remain intact.

Two platform-targeted H.264 VBV ceilings are represented in the existing `PlatformConfig`: X's 720p 8 Mbps upper ceiling, and Instagram Reels Publishing API's 25 Mbps upper ceiling. FFmpeg receives `-maxrate` and `-bufsize` together with CRF, rather than conflicting CRF with a fixed average `-b:v` target. These ceilings constrain bitrate peaks; they do **not** guarantee that average bitrate will equal the platform's recommended upload bitrate.

This is intentionally a conservative H.264/MP4-oriented update. The app's current output-format control is global, not preset-specific; selecting WebM still selects the existing VP9/Opus path. The platform H.264 profile and H.264-only VBV ceiling are therefore omitted for WebM output. The app does not upsample or force a new frame rate: it keeps source FPS.

## Built-in settings

| Preset | Output canvas | CRF / quality label | x264 preset | Audio bitrate | Additional platform parameters |
|---|---:|---|---|---:|---|
| YouTube | 1920×1080 (16:9) | 18 / `high` | `fast` | 384 kb/s | H.264 High Profile, 4:2:0, 48 kHz audio |
| YouTube Shorts | 1080×1920 (9:16) | 18 / `high` | `fast` | 384 kb/s | Same upload encoding guidance as YouTube; vertical canvas |
| Instagram Square | 1080×1080 (1:1) | 20 / `good` | `fast` | 128 kb/s | H.264 High Profile, 4:2:0, 48 kHz audio |
| Instagram Reels | 1080×1920 (9:16) | 20 / `good` | `fast` | 128 kb/s | H.264 VBV `-maxrate 25M -bufsize 25M`; 48 kHz AAC |
| TikTok | 1080×1920 (9:16) | 20 / `good` | `fast` | 160 kb/s | H.264 High Profile, 4:2:0, 48 kHz audio |
| X | 1280×720 (16:9) | 22 / `good` | `fast` | 128 kb/s | H.264 VBV `-maxrate 8M -bufsize 16M`; 48 kHz AAC |
| Reddit | 1200×1500 (4:5 portrait) | 22 / `good` | `fast` | 128 kb/s | H.264 High Profile, 4:2:0, 48 kHz audio |

**Common FFmpeg behavior:** H.264 output uses `libx264`; a `.webm` output uses the existing `libvpx-vp9` + `libopus` path. Platform exports with audio add `-ar 48000`; H.264 platform exports add `-profile:v high`. `-pix_fmt yuv420p` and MP4 `-movflags +faststart` are already part of the builder and remain so. Audio channels are not forcibly downmixed. If audio removal is enabled, audio codec/bitrate/sample-rate flags are not emitted.

The values under `qualityPreset` are UI representatives, not a separate FFmpeg control. The application's Rust quality table maps CRF 18 to `high` and CRF values 20–22 to `good`; this makes each built-in label consistent with its actual CRF. Rust remains the final authority for transient encoding overrides.

## Platform-by-platform reasoning

### YouTube and YouTube Shorts

**Official guidance:** MP4; H.264 High Profile; progressive video; 4:2:0; source-matched frame rate; AAC-LC or Opus at 48 kHz. YouTube publishes reference upload bitrates—8 Mb/s for 1080p at 24/25/30 fps and 12 Mb/s at 48/50/60 fps—but says a bitrate limit is not required. That makes CRF a better default than a rigid average bitrate target for this quality-oriented workflow.

**Preset choice:** CRF 18 is a high-quality H.264 setting; `fast` favors turnaround time over `slow` while remaining a good compression preset. AAC at 384 kb/s is intentionally aligned with YouTube's published stereo audio recommendation. Shorts use the same encoding baseline with a 1080×1920 vertical canvas. No video `-maxrate` is attached because YouTube explicitly does not require a bitrate cap and its reference average bitrates should not be copied into CRF as if they were limits.

**Trade-off:** Compared with `slow`, `fast` generally completes sooner but may produce a larger file at comparable subjective quality. CRF does not promise a file size or specific average bitrate.

### Instagram Square and Instagram Reels

**Official guidance and limits:** Meta's Instagram Reels Publishing API specification lists MOV/MP4, H.264 or HEVC, AAC at 48 kHz, 23–60 fps, recommended 9:16, a maximum 25 Mb/s video bitrate and 128 kb/s audio bitrate. This is explicitly the **Reels API upload** specification; it must not be presented as a universal hard limit for every organic upload route. The square preset retains a conventional 1080×1080 canvas; Reels uses 1080×1920.

**Preset choice:** CRF 20 is a quality/file-size compromise for 1080p social video. AAC at 128 kb/s and 48 kHz matches the API document. Reels gets a 25 Mb/s VBV ceiling with a 25 Mb buffer; the square preset does not inherit that ceiling as the cited limit is tied to Reels API guidance. `fast` reduces encode time.

**Trade-off:** Highly detailed or noisy footage may hit the Reels VBV ceiling and receive more quantization than unconstrained CRF would, which is the intended exchange for staying under a bitrate peak limit. This parameter limits peaks, not average bitrate.

### TikTok

**Official guidance located:** TikTok's browser-upload help says MP4 or WebM, 720×1280 or higher, up to 30 minutes, and below 10 GB. The public organic-upload help does not establish an official CRF, target video bitrate, AAC bitrate, or universal sample-rate requirement. TikTok's exact requirements can differ for ad inventory and API integrations.

**Preset choice:** 1080×1920 meets the documented 720×1280 minimum and the common vertical 9:16 workflow. CRF 20 and `fast` are app-side quality/performance choices, not TikTok-mandated values. AAC at 160 kb/s is a conservative application choice; it is not represented as an official TikTok bitrate requirement. The app's default MP4 is a suitable broad-compatibility choice, while WebM remains available if selected globally.

**Trade-off:** Compared with CRF 18, CRF 20 usually reduces file size/encode work for social-video material. The actual size is source-dependent; the 10 GB upload limit is not guaranteed by CRF alone.

### X

**Official guidance:** X Media Studio recommends 1280×720 landscape, H.264/AVC, 5–8 Mb/s video and AAC-LC stereo/mono, and supports up to 60 fps. Separate X Ads creative specs recommend 5–8 Mb/s for 720p; ad specifications are not automatically general-post requirements.

**Preset choice:** 1280×720 is enforced instead of being shown as a nominal resolution while the layout is calculated from the source. CRF 22 plus `-maxrate 8M -bufsize 16M` retains CRF-based quality control while limiting peaks to the upper end of X's published 720p bitrate range. This is not a fixed average bitrate target, so average bitrate may be below 5 Mb/s on simple content or still vary with complexity. AAC at 128 kb/s and 48 kHz is the app's compatibility-oriented choice; X specifies AAC-LC but does not require this exact audio bitrate in its Media Studio page.

**Trade-off:** The ceiling may raise quantization on very complex/high-motion clips; in exchange it reduces excessive bitrate peaks. If a future dedicated ad-export mode is added, frame-rate capping/normalization should be a separate explicit policy: current exports preserve the source frame rate rather than silently changing it.

### Reddit

**Documentation limitation:** Reddit's public general-organic video publishing documentation does not provide a single comprehensive, current encoder profile/bitrate table. Reddit Ads' official free-form creative specification lists MP4/MOV, a 1 GB maximum file size, maximum 30 fps, and recommends 1200×1500 for portrait 4:5. Those are **advertising creative specifications**, not a claim that every organic Reddit post is required to use 1200×1500 or 30 fps.

**Preset choice:** The 1200×1500 4:5 canvas follows Reddit Ads' documented portrait recommendation and is a useful portrait option, but the preset is labelled to make that aspect ratio clear. CRF 22, `fast`, and 128 kb/s audio are application engineering defaults because there is no equivalent official organic-post encoding table for these settings. The source frame rate is preserved; the preset does not silently impose the ads-only 30 fps restriction.

**Trade-off:** CRF 22 favors smaller social uploads over CRF 18. Since file size depends on content, users targeting an ad's 1 GB limit should still verify the rendered file before upload.

## FFmpeg implementation details

- **CRF is not a target bitrate.** The app keeps its existing CRF workflow. No `-b:v` average bitrate is added to those jobs.
- **VBV options are paired.** A configured platform cap emits both `-maxrate` and `-bufsize`; validation rejects an incomplete pair, zero, or malformed values before rendering. They are applied only for `libx264`, not to the VP9 path.
- **Speed:** all seven built-ins move from x264 `slow` to `fast`. This is a deliberate turnaround-time optimization; actual speedup depends on source, CPU, filter work (subtitles/blur/overlays), and concurrent batch load.
- **Pixel format/profile:** the existing output pixel format `yuv420p` is preserved. `-profile:v high` is explicit for H.264 platform jobs; X explicitly permits Baseline, Main, or High with 4:2:0, and YouTube lists High.
- **Audio:** 48 kHz is emitted only for platform jobs with audio. The existing AAC/Opus codec selection remains bound to output extension. Channel count is preserved rather than forcing stereo, since YouTube permits stereo or stereo+5.1 and X accepts stereo/mono.
- **FPS:** no frame-rate conversion is introduced; the existing path keeps the source frame rate. This aligns with YouTube's source-matched guidance and avoids low-value frame duplication.
- **Container:** `+faststart` remains enabled for MP4. The output-format chooser is global today, not attached to individual platform presets. The built-ins therefore do not silently override a user's format choice; for the strongest documented upload compatibility, leave the app's output format at MP4. WebM intentionally uses the app's VP9/Opus implementation, and H.264-only options are omitted.

## Files changed

1. `src-tauri/resources/presets/platform_specific_presets.json` — preset CRF/quality labels, faster speed choice, platform canvases, selected VBV limits, and clearer Reddit description.
2. `src-tauri/src/video/types.rs` — backward-compatible optional `video_max_rate` and `video_buffer_size` fields on `PlatformConfig`.
3. `src-tauri/src/video/validation.rs` — paired-value syntax/range validation and unit tests.
4. `src-tauri/src/video/ffmpeg_args_builder.rs` — platform H.264 profile, 48 kHz audio, codec-scoped VBV flags, and argument-builder regression tests.
5. `src-tauri/src/video/scheduler.rs` — update an existing `PlatformConfig` test initializer for the new fields.
6. `src/types/backend.ts` — synchronize the generated TypeScript `PlatformConfig` shape.
7. `docs/platform-export-presets.md` — this research/implementation report.

No unrelated frontend architecture, render scheduling, default-output-format, or FPS behavior was changed.

## Official sources

Platform documentation is the authority for platform-facing statements; ad and API specs are identified as such above.

- [YouTube recommended upload encoding settings](https://support.google.com/youtube/answer/1722171?hl=en) — MP4/fast start, H.264 High Profile, 4:2:0, source-matched frame rate, audio sample rate and reference video bitrates.
- [X Media Studio FAQs](https://help.x.com/en/using-twitter/media-studio-faqs.html) — H.264/AVC, MP4/MOV, recommended 720p/5–8 Mb/s, frame-rate maximum, AAC-LC.
- [X Ads creative specifications](https://help.x.com/en/business-and-advertising/creative-ad-specifications) — corroborating 720p ad bitrate range and codec/profile/pixel-format details; used as ad-specific context only.
- [TikTok Creator tools on TikTok](https://support.tiktok.com/en/using-tiktok/creating-videos/creator-tools-on-tiktok) — browser uploads must be MP4/WebM, at least 720×1280, up to 30 minutes and less than 10 GB.
- [Meta Instagram Reels Publishing API documentation (Postman collection)](https://www.postman.com/meta/instagram/folder/f95kq5e/reels-publishing) — API-specific container, video/audio codecs, 48 kHz, 23–60 fps, 9:16 recommendation, maximum video bitrate and audio bitrate.
- [Reddit Ads free-form ad specifications](https://business.reddithelp.com/s/article/free-form-ad-specifications) — ad-specific MP4/MOV, maximum file size/FPS and recommended portrait canvas.
- [FFmpeg codecs documentation — libx264](https://ffmpeg.org/ffmpeg-codecs.html) — CRF, preset, profile, VBV-related options.
- [FFmpeg formats documentation — MOV/MP4](https://ffmpeg.org/ffmpeg-formats.html) — `+faststart` moves MP4/MOV metadata to the start of the file.

## Verification status

**Static checks planned/performed for this review:** preset JSON parses; the seven known IDs are retained; CRF and quality labels match the backend mapping; dimensions match the declared aspect ratios; VBV settings occur only as a pair and only on the intended H.264 paths; and all Rust `PlatformConfig` struct literals are updated for the new optional fields.

**Not yet verified in an executable build environment:** `cargo test`, the Tauri/frontend build, real FFmpeg renders for every preset, `ffprobe` metadata inspection, A/V sync checks, or uploads to platform services. The new Rust unit tests are included in the review branch but have not been executed here. Those runtime checks are required before merging or treating this as release-ready.
