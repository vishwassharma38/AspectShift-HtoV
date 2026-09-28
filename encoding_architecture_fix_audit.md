# Encoding Architecture Hardening — Audit Report

Patch task: `prompt.md` — *AspectShift-HtoV Encoding Architecture Hardening & Remaining-Issues Fix*
(Sections cited below as "Issue #N" refer to that task document; "§21" refers
to its Definition of Done.)

This report is written so an independent engineer (or ChatGPT review pass) can
verify the patch without repeating the full investigation. Every behavioral
claim below is tied to a file/function and, where applicable, to an executed
test name. All verification commands were run in the working tree described
in §H; no Git mutation was performed at any point.

---

## A. Executive Summary

**Investigated:** the full encoding data flow — preset JSON → Rust loaders +
validation → frontend selection/override state → `OutputJob` IPC →
`ResolvedJob` construction → `render_single` → FFmpeg builder / `-c copy`
passthrough — plus quality-authority semantics, custom-preset persistence,
existing tests, and working-tree state. Investigation preceded every edit
(Rule 1); no file was modified until the data flow and the task's assumptions
were confirmed against the code.

**Found:** the task's assumptions matched the code in all nine issues. The
working tree already contained the authoritative Rust resolver
(`resolve_effective_encoding`, `EncodingOverrides`, `QualityAuthority`) but
**production never called it**: the frontend resolved effective encoding
locally (`resolveEffectiveEncoding`) and Rust blindly trusted the received
profile, with `forceReencode` threaded as frontend-provided state. The same
duplication covered quality levels, CRF bands, speed/bitrate lists, and
authority rules. `ResolvedJob` was built by direct struct literals at three
sites (two render, one layout) plus preview.

**Changed:** Rust is now the final authority at the render boundary
(validate → resolve → derive intent → `ResolvedJob::resolve_for_render`);
canonical semantics are exposed via two Tauri commands
(`get_encoding_metadata`, `resolve_encoding_preview`); the frontend consumes
metadata + backend preview and owns no resolution rules; re-encode intent is
derived, never trusted; the resolver is fallible; speed/audio applicability is
explicit in UI + preview warnings + render logs; custom-preset snapshot
semantics verified and documented. No FFmpeg process changes. No preset JSON
changes. No Git operations.

**Result:** `cargo test` 204/204 pass (12 new tests: 8 in `encoding.rs`, 4 in
`types.rs`), `npx tsc --noEmit` clean, `cargo clippy --all-targets` introduces
no new warnings, `npx vite build` succeeds. The intended architecture of
prompt §1 (Rust as final authority; frontend expresses intent) is achieved
within the constraints below.

**Major deviations from the proposed solution** (all deliberate, minimal-scope;
each is expanded with rationale in §D):

1. `OutputJob.force_reencode` is **retained but ignored** on intake instead of
   removed (backward-compatible deserialization; smaller diff).
2. `normalize_targets` still validates (baseline + overrides) and
   `resolve_for_render` re-validates — double validation as defense in depth
   rather than a single gate.
3. Preview warnings are a string list, not a dedicated diagnostic type.
4. `ResolvedJob::for_layout` keeps an `input_path` parameter (logo-aware
   subtitle layout preserved exactly).
5. `ManualCrf` without a display name now **derives** the band label (small
   extension so the frontend never needs the mapping table).
6. `Baseline` authority carrying quality values is now an **error** (was
   silently kept) — intentional hardening per Issue #7.

---

## B. Issues Addressed

### Issue #1 — Backend resolver is not the production authority

- **Original issue:** effective encoding resolved in TypeScript; Rust trusted
  the received profile instead of resolving authoritative state itself.
- **Applied to this repo:** yes — confirmed by tracing `handleStartBatch` →
  `OutputJob.encoding` (effective) → verbatim clone into `ResolvedJob` in
  both render paths, with zero production callers of the Rust resolver.
- **Actual root cause:** the render contract was
  `OutputJob{effective encoding, frontend forceReencode}`; the backend had no
  baseline/override inputs to resolve from.
- **Affected files/components:** `src-tauri/src/video/types.rs`
  (`OutputJob.encoding_overrides`, `ResolvedJob::resolve_for_render` /
  `for_layout`), `src-tauri/src/video/encoding.rs` (serializable overrides,
  fallible resolver), `src-tauri/src/video/mod.rs` (`convert_to_ratio`),
  `src-tauri/src/video/batch_processor.rs` (render construction),
  `src/App.tsx` (sends baseline + overrides), `src/types/backend.ts`
  (regenerated).
- **Data flow before:** frontend resolve → `OutputJob{effective, forceReencode}`
  → blind clone. **After:** frontend intent → `OutputJob{baseline, overrides}`
  → Rust `resolve_for_render` → `render_single`.

### Issue #2 — Rust/TypeScript duplicate business logic

- **Original issue:** quality levels, representative CRFs, CRF bands,
  resolution logic, authority rules, speed/bitrate definitions duplicated
  across the boundary with tests as the only sync mechanism.
- **Applied:** yes — all seven duplicated items were present in `App.tsx`.
- **Actual root cause:** no IPC surface exposed canonical semantics, so the
  frontend re-implemented them.
- **Affected files/components:** `encoding.rs` (`build_encoding_metadata`,
  `preview_effective_encoding`, `get_encoding_metadata` /
  `resolve_encoding_preview` commands), `export_types.rs`, `lib.rs`,
  `capabilities/default.json`, `backend.ts`, `App.tsx` (all duplicates
  deleted; controls render from metadata; display comes from the preview
  command with 120 ms debounce for slider movement).
- **Data flow:** `get_encoding_metadata` at startup → option lists;
  `resolve_encoding_preview(baseline, overrides, outputFormat, removeAudio)` →
  displayed effective profile + warnings, using production resolution.
- **Preset-specific baselines preserved:** `Baseline` authority keeps stored
  values untouched; validation checks membership/bounds only, never CRF↔name
  consistency (e.g. `instagram` crf 20 / `high` remains valid).

### Issue #3 — FFmpeg process/output verification gap

- **Original issue / required action:** none functionally — document manual
  verification; distinguish coverage gap from runtime failure.
- **Applied:** documentation only (no code change to the FFmpeg pipeline;
  `ffmpeg_args_builder.rs` and `ffmpeg.rs` untouched; `convert.rs` gained two
  additive `info!` diagnostic logs only).
- **Determination:** absence of automated end-to-end coverage is recorded as a
  coverage gap, not as evidence of failure. The task's real-world verification
  claim is accepted as stated; nothing in this patch contradicts it.

### Issue #4 — `forceReencode` manually threaded

- **Original issue:** `forceReencode: OutputJob → BatchJob → ResolvedJob →
  render_single`, fragile against future constructors omitting/resetting it.
- **Applied:** yes — confirmed the exact chain plus a second hardcoded `false`
  for the subtitle-layout job.
- **Actual root cause:** derived intent modeled as independent user data.
- **Affected files/components:** `types.rs` (`resolve_for_render` derives via
  `has_explicit_encoding_intent`, ignores intake `force_reencode`;
  `for_layout` for non-render paths), `batch_processor.rs`, `mod.rs`.
- **Invariant now:** no caller can pair `forceReencode=false` with encoding
  changes — Rust derives it from the actual overrides (proven by
  `render_boundary_*` tests in both directions, §F).

### Issue #5 — Speed preset silently inapplicable to WebM/VP9

- **Original issue:** `Speed = Fast` + `Output = WebM` implies application
  while the builder correctly omits `-preset` for VP9.
- **Applied:** yes, presentation + diagnostics scope (no encoder redesign, no
  VP9 preset invented, builder untouched).
- **Affected files/components:** `encoding.rs` (`CodecCapability` metadata,
  preview warning), `convert.rs` (render log when `.webm` output omits
  `-preset`), `App.tsx` (speed select disabled for WebM with "not applicable"
  note + preview warnings rendered).

### Issue #6 — Audio bitrate irrelevant when `remove_audio`

- **Original issue:** `remove_audio = true` → `-an`, so the bitrate control
  misleads; prior selection should survive re-enable.
- **Applied:** yes, UI state scope (backend `-an` behavior untouched).
- **Affected files/components:** `App.tsx` (bitrate select disabled while
  Remove Audio is on, with preservation note;
  `manualEncodingOverrides.audioBitrate` is never cleared so re-enabling
  restores the previous selection), `encoding.rs` (preview warning),
  `convert.rs` (render log).

### Issue #7 — Invalid values reach validation too late

- **Original issue:** unknown/invalid values pass through resolution, rejected
  only later; desired `Result<EncodingProfile, EncodingError>` boundary.
- **Applied:** yes — confirmed the infallible resolver with silent fallbacks
  (unknown quality → keep baseline CRF; `ManualCrf` without CRF → keep
  baseline; `Baseline` + stray quality → silently kept).
- **Actual root cause:** no fail-fast at the intent layer; validation ran on
  the already-resolved profile far from the render boundary.
- **Affected files/components:** `encoding.rs`
  (`resolve_effective_encoding` → `Result<EncodingProfile, VideoError>`;
  `validate_encoding_overrides`; shared validators `validate_quality_name` /
  `validate_speed_name` / `validate_crf_value` / `validate_audio_bitrate_value` /
  `validate_baseline_profile`), `validation.rs` (delegates;
  `validate_output_job` also validates overrides), `types.rs`
  (`resolve_for_render` fails before any job exists; batch path marks a
  deterministic job failure).
- **Boundary now:** `input → validate → resolve → ResolvedJob`, with invalid
  input producing no job. No giant validator, no new enums (smallest change;
  exact `VideoError::InvalidInput` type reused per "exact types must match the
  existing architecture").

### Issue #8 — Transient overrides vs custom preset snapshots

- **Original issue:** determine whether custom presets should snapshot or
  reference; task states current behavior may be intentional.
- **Determination:** current behavior is correct snapshot semantics —
  confirmed, not a bug. `CustomPreset{ id, name, ratio, encoding }` carries no
  parent reference; `handleSavePreset` stores `deepClone(encodingState)` (the
  backend-resolved effective configuration as displayed). Built-in changes can
  never mutate a saved snapshot; transient overrides are not persisted.
- **Work done:** clarifying comment at the save site; no inheritance /
  versioning / redesign (per task). Covered in regression §G.

### Issue #9 — Uncommitted implementation / Git prohibition

- **Original issue/required action:** document repository state instead of any
  Git operation.
- **Action:** documentation only; **zero Git mutations performed** (no commit,
  push, pull, merge, rebase, checkout, reset, branch — only read-only `status`
  / `diff` / `--list` inspection).
- **Recorded state:** the tree was dirty before this task (9 staged entries
  implementing the encoding/override model atop `HEAD 1157721`) and this task
  adds unstaged modifications on top; see §H. `encoding.rs` remains
  staged-new (`AM`).

---

## C. Root Cause Analysis

### #1 — Frontend-resolved authority

- **Observed behavior:** the profile reaching FFmpeg was computed by
  TypeScript; Rust could not distinguish user intent from resolved output.
- **Relevant implementation:** `App.tsx handleStartBatch` (per-target
  `resolveEffectiveEncoding`) → `OutputJob.encoding` →
  `batch_processor.rs process_batch_job`
  (`encoding: job.output.encoding.clone()`) and `mod.rs convert_to_ratio`
  (`encoding: job.encoding`) → `ResolvedJob`.
- **Root cause:** the IPC contract carried answers, not intent; the Rust
  resolver existed but was unreachable from production.
- **Why vulnerable:** any divergence between the two resolvers (or any new
  IPC producer) silently changed renders; the backend could not enforce its
  own semantics.

### #2 — Duplicated semantics

- **Observed:** seven encoding concepts implemented twice
  (`QUALITY_LEVELS`, both mapping directions, resolution, authority,
  speed/bitrate lists).
- **Relevant implementation:** `App.tsx` constants/functions vs
  `encoding.rs` constants/functions, synced only by comments and unit tests.
- **Root cause:** no canonical-metadata IPC surface existed.
- **Why problematic:** future edits to one side (e.g. a new quality level)
  would silently desynchronize display from rendering.

### #4 — Threaded intent flag

- **Observed:** `force_reencode` copied through four structs with a parallel
  hardcoded `false` for layout.
- **Relevant implementation:** `OutputJob.force_reencode` →
  `BatchJob.output` → `ResolvedJob.force_reencode` →
  `is_passthrough_allowed(..., force_reencode)`.
- **Root cause:** derived state modeled as primary data.
- **Why vulnerable:** a new `ResolvedJob` literal (or a stale `false`) would
  re-enable `-c copy` despite explicit encoding changes — silent data loss of
  user intent.

### #5 — VP9 speed silence

- **Observed:** speed control active for a container whose codec ignores it.
- **Relevant implementation:** `ffmpeg_args_builder.rs supports_preset`
  (`libx264`/`libx265` only) vs unconditional speed UI.
- **Root cause:** applicability knowledge lived only in the builder.
- **Why problematic:** UI implied an effect the renderer correctly never
  produced.

### #6 — Remove-audio bitrate

- **Observed:** bitrate control active while `-an` made it meaningless.
- **Relevant implementation:** builder's `remove_audio → -an` branch vs
  unconditional bitrate UI.
- **Root cause:** same as #5 — applicability not surfaced.
- **Why problematic:** misleading control; risk of discarding the user's
  selection on toggle.

### #7 — Late validation

- **Observed:** invalid intent produced a valid-looking profile.
- **Relevant implementation:** infallible `resolve_effective_encoding` with
  `Option`-fallback branches.
- **Root cause:** no validation of the intent layer; errors surfaced (if at
  all) far downstream.
- **Why vulnerable:** silent fallbacks mask caller bugs and user-input
  corruption alike.

---

## D. Fixes Applied

### D1. Serializable intent + authoritative render boundary (Issues #1, #4)

**Files:** `src-tauri/src/video/encoding.rs`, `src-tauri/src/video/types.rs`,
`src-tauri/src/video/mod.rs`,
`src-tauri/src/video/batch_processor.rs`, `src/App.tsx`,
`src/types/backend.ts`.

- `QualityAuthority` and `EncodingOverrides` gained
  `Serialize/Deserialize/Type` with `camelCase` serde
  (`encoding.rs`; wire names `baseline/qualityPreset/manualCrf` match the
  previous frontend union exactly).
- `OutputJob` gained `encoding_overrides: EncodingOverrides`
  (`#[serde(default)]`; `types.rs`). `encoding` is now documented as the
  canonical **baseline**, not the effective profile.
- `ResolvedJob::resolve_for_render(job_id, session_id, input/output paths,
  output, subtitles, threads)` (`types.rs`): `validate_output_job` →
  `resolve_effective_encoding` → `has_explicit_encoding_intent` → construct.
  Intake `force_reencode` is ignored (documented on the field).
- `ResolvedJob::for_layout(id, input_path, ratio, encoding, effects,
  platform_config)` (`types.rs`): unvalidated, `force_reencode=false`,
  for preview + subtitle measurement only.
- `convert_to_ratio` (`mod.rs`) and batch render (`batch_processor.rs`)
  call `resolve_for_render`; preview (`mod.rs`) and subtitle layout
  (`batch_processor.rs`) call `for_layout`. Invalid batch input yields a
  deterministic per-job failure (mirrors the adjacent subtitle-error path;
  `classify_video_error(InvalidInput) = Deterministic`), never an FFmpeg
  spawn.
- Frontend `handleStartBatch` sends `encoding: deepClone(baseline)` +
  `encodingOverrides` snapshot per target (platform, custom, aspect-ratio
  alike); `forceReencode` is no longer sent or computed.
- **Why it fixes the root cause:** there is no longer any channel for a
  pre-resolved profile to enter rendering — the only render constructors
  resolve from baseline + intent themselves.
- **Correspondence:** implements the §3 contract
  (frontend baseline + transient overrides → Rust validate + resolve + derive
  intent → `ResolvedJob` → `render_single`) using the existing resolver and
  existing command patterns.

New IPC contract (wire shape):

```jsonc
// OutputJob.encoding = canonical baseline (preset's tuned values)
{ "crf": 20, "qualityPreset": "high", "speedPreset": "slow", "audioBitrate": "160k" }
// OutputJob.encodingOverrides = transient intent (this session only)
{ "qualityPreset": null, "crf": 24, "speedPreset": null, "audioBitrate": null, "qualityAuthority": "manualCrf" }
// OutputJob.forceReencode = legacy, ignored on intake, recomputed in Rust
```

### D2. Canonical metadata + backend preview (Issue #2)

**Files:** `encoding.rs` (`SPEED_PRESETS`, `AUDIO_BITRATE_CANDIDATES`,
`QualityLevelMeta`, `CodecCapability`, `EncodingMetadata`,
`build_encoding_metadata`, `EncodingPreviewRequest/Response`,
`preview_effective_encoding`, `get_encoding_metadata`,
`resolve_encoding_preview`), `export_types.rs`, `lib.rs`,
`capabilities/default.json`, `backend.ts`, `App.tsx`.

- Bands are computed from `QUALITY_LEVELS` midpoints by construction, so
  metadata cannot disagree with `quality_for_crf` (proven by
  `metadata_bands_cover_full_slider_and_match_derivation`, which tiles 0–51
  and round-trips every representative).
- `resolve_encoding_preview` runs **production** `resolve_effective_encoding`
  plus non-fatal applicability warnings — one rule implementation, two
  call sites (preview + render).
- Frontend deleted `QUALITY_LEVELS`, `qualityCrfForPreset`, `qualityForCrf`,
  `resolveEffectiveEncoding`, `hasExplicitEncodingIntent`, `SPEED_PRESETS`,
  `AUDIO_BITRATE_CANDIDATES` (≈120 lines). Option lists render from metadata
  (small local fallbacks cover first paint only). Displayed encoding always
  comes from the preview command: `activeBaseline` memo → debounced effect
  (120 ms trailing; discrete quality changes request immediate refresh);
  slider thumb binds to the pending override for instant feedback while the
  label follows the authoritative preview.
- **Performance:** continuous slider movement issues at most one trailing IPC
  per 120 ms pause; no new state architecture (two refs + existing
  `useState`/`useEffect`).
- **Correspondence:** implements §4 (`Rust = canonical semantics`, preview
  uses the same resolver as production, debounce/slider-release performance
  rule, preset-baseline distinction preserved).

### D3. Fallible resolution + shared validators (Issue #7)

**Files:** `encoding.rs`, `validation.rs`, `types.rs`, `batch_processor.rs`.

- `resolve_effective_encoding` → `Result<EncodingProfile, VideoError>`;
  validates baseline, then overrides (`validate_encoding_overrides`: value
  domains + authority combinations), then re-validates the effective profile.
- Single-source validators moved to `encoding.rs`
  (`validate_quality_name`, `validate_speed_name`, `validate_crf_value`,
  `validate_audio_bitrate_value`, `validate_baseline_profile`);
  `validation.rs::validate_encoding_profile` delegates and
  `validate_output_job` additionally validates overrides. Dependency direction
  is one-way (`validation → encoding`); no cycle.
- Rejected combinations: unknown quality/speed/bitrate values; `QualityPreset`
  without a name; `ManualCrf` without a CRF; `Baseline` carrying quality
  values; out-of-range baselines.
- `ManualCrf` without an explicit display name derives it via
  `quality_for_crf` (replacing the old stale-label retention).
- **Correspondence:** implements §9 (`Result`-returning resolver, reuse of
  existing validation, no new enums, validate→resolve→`ResolvedJob`).

### D4. Applicability surfaces (Issues #5, #6)

**Files:** `encoding.rs` (capabilities + preview warnings), `convert.rs`
(two additive `info!` logs), `App.tsx` (disabled states + notes + warning
list).

- WebM: speed `<select disabled>` + "Speed preset is not applicable to
  VP9/WebM."; preview warning; render log on `-preset` omission.
- Remove-audio: bitrate `<select disabled>` + preservation note; override
  value retained in state (restore path is "do nothing on toggle" —
  verified by inspection: no handler clears `audioBitrate`); preview warning;
  render log on `-b:a` omission.
- Backend encode behavior unchanged (`-preset` still omitted for VP9, `-an`
  still suppresses `-b:a`).

### D5. Snapshot documentation (Issue #8)

**Files:** `App.tsx` (`handleSavePreset` comment). No behavioral change:
`CustomPreset` has no parent reference; the saved `encoding` is the
backend-resolved effective snapshot; overrides are session-only and never
persisted.

### Proposed vs implemented (per §19-D requirement)

| # | Proposed approach | Implemented approach | Reason for difference |
|---|---|---|---|
| 1 | Rust authority at boundary | Same, via `resolve_for_render` | — (as proposed) |
| 2 | Expose metadata; preview via same resolver | Same, plus string-list warnings | Smallest diagnostic shape |
| 4 | Derive intent; centralize construction | Same; intake `force_reencode` retained-but-ignored | Backward-compatible deserialization; avoids breaking old payloads/tests |
| 4 | Single validation gate | Validate in `normalize_targets` AND `resolve_for_render` | Defense in depth; second gate is the authority boundary |
| 5/6 | Target-aware applicability | Disabled UI + preview warnings + render logs | No new types; backend semantics untouched |
| 7 | Fallible resolver, reuse validation | Same; validators hoisted into `encoding.rs` | One-way dependency; single source of truth |
| 8 | Preserve snapshots | Same; comment only | Confirmed not a bug |

---

## E. Architecture & Data Flow

Final production flow (all video-producing paths):

```text
Built-in preset JSON (compile-time include_str! + validation)
        │
        ▼
Frontend receives canonical presets + encoding metadata
        │
        ▼
Frontend stores USER INTENT ONLY
(selected baseline ref + transient EncodingOverrides + effects)
        │
        │ render request: OutputJob{ baseline, encodingOverrides, effects }
        ▼
Rust render boundary (ResolvedJob::resolve_for_render)
  validate baseline + overrides (validate_output_job)
        ↓
  resolve_effective_encoding()  ← single authority (Result)
        ↓
  has_explicit_encoding_intent() → force_reencode (derived)
        ↓
ResolvedJob { effective encoding, derived intent, render settings }
        │
        ▼
render_single()
   ├── passthrough allowed? (geometry/effects AND !force_reencode)
   │       └── yes → `-c copy` stream copy
   └── no → FFmpeg argument builder (resolved CRF/speed/bitrate)
```

Preview flow (display only, never rendered):

```text
frontend user intent (baseline + overrides + outputFormat + removeAudio)
        ↓
resolve_encoding_preview  →  same resolve_effective_encoding()
        ↓
{ effective, warnings[] } → frontend display + notes
```

Responsibilities after the patch (prompt §12 model):

- **Frontend owns:** user interaction, selected preset/baseline reference,
  transient overrides, UI state, rendering backend metadata, displaying
  preview values + warnings, applicability presentation (disabled states).
  It owns **no** resolution, mapping, band, intent, validation, or FFmpeg
  semantics (verified: no local copies remain — `grep` for
  `resolveEffectiveEncoding|qualityForCrf|QUALITY_LEVELS|SPEED_PRESETS|
  AUDIO_BITRATE_CANDIDATES|hasExplicitEncodingIntent` in `src/` returns only
  `backend.ts` generated entries).
- **Rust owns:** preset loading/validation, encoding definitions, resolution,
  validation, derived intent, `ResolvedJob` construction, render-time
  authority, FFmpeg args, passthrough decision.
- **Presets** supply reusable canonical baselines (preset-specific CRFs
  intact). **Overrides** supply transient intent. **Preview** supplies display
  values through production rules. **Encoding authority resides in
  `video::encoding`** (`resolve_effective_encoding` + validators +
  `QUALITY_LEVELS`), consumed by exactly two entry points
  (`resolve_for_render`, `preview_effective_encoding`).
- Unrelated responsibilities were not moved: filter graph, scheduling,
  probing, thumbnails, subtitles, auth, and config are untouched.

---

## F. Behavior Verification

Test/format key: each item lists the verification performed, expected vs
observed behavior, and result. Command outputs are quoted verbatim.

### #1 — Rust authority

- **Verification:** code trace (both render paths call `resolve_for_render`;
  no remaining direct render `ResolvedJob` literals — `grep ResolvedJob{`
  returns only `for_layout` + tests) + `render_boundary_*` type tests +
  full suite.
- **Expected:** no render job bypasses Rust resolution.
- **Observed:** `convert_to_ratio` and batch `process_batch_job` both route
  through `resolve_for_render`; subtitle/preview use `for_layout`
  (never rendered).
- **Result:** PASS.

### #2 — No duplicated authority

- **Verification:** `grep` for deleted symbols (clean except generated
  `backend.ts`); `tsc`; `vite build`; metadata/preview tests.
- **Expected:** frontend compiles and renders controls with zero local rules.
- **Observed:** `tsc --noEmit` clean; `vite build` succeeds (72 modules,
  JS bundle renders); dropdown/speed/bitrate options derive from metadata;
  `metadata_bands_cover_full_slider_and_match_derivation` proves
  metadata≡resolution.
- **Result:** PASS (static + build level; live IPC round-trip not exercised —
  see Not verified).

### #4 — Derived intent

- **Verification:** `render_boundary_resolves_baseline_plus_overrides_and_derives_intent`
  (intake `false` + overrides → derived `true`) and
  `render_boundary_without_overrides_keeps_baseline_and_passthrough`
  (intake `true` + no overrides → derived `false`).
- **Expected:** intake flag ignored in both directions.
- **Observed:** as expected.
- **Result:** PASS. Passthrough-eligible baseline path and forced re-encode
  path both proven at the constructor level.

### #5 — WebM speed

- **Verification:** `metadata_…` asserts `webm.supports_preset == false`;
  `preview_…` asserts the VP9/WebM warning; builder tests assert no `-preset`
  for VP9; UI disabled state verified statically.
- **Expected:** no inappropriate flag; explicit UI.
- **Observed:** as expected.
- **Result:** PASS (unit + static; visual rendering not exercised).

### #6 — Remove-audio bitrate

- **Verification:** `preview_…` asserts the Remove Audio warning;
  `remove_audio_produces_an_without_audio_bitrate` asserts `-an` without
  `-b:a`; UI preserves `audioBitrate` override (no clearing handler exists).
- **Expected:** disabled UI, preserved selection, `-an` backend.
- **Observed:** as expected.
- **Result:** PASS (unit + static).

### #7 — Invalid input fails fast

- **Verification:** 5 new tests (`unknown_quality…`, `unknown_speed…`,
  `malformed_audio…` incl. `loud/64/8k/1024k`, `mismatched_authority…`,
  `invalid_baseline…`) + `render_boundary_rejects_invalid_overrides…`
  (no job constructed) + batch deterministic-failure path (code trace to
  `classify_video_error → Deterministic`).
- **Expected:** `Err` before any `ResolvedJob`; batch marks job failed.
- **Observed:** as expected.
- **Result:** PASS.

### #8 — Snapshot semantics

- **Verification:** struct inspection (`CustomPreset` has no parent field) +
  save-path inspection (stores effective snapshot; overrides session-only).
- **Expected:** built-in changes cannot mutate saved presets.
- **Observed:** as expected by construction.
- **Result:** PASS (static).

### Full command results

- `cargo test --manifest-path src-tauri/Cargo.toml` → **204 passed, 0 failed**
  (lib suite; bins + doctests 0 tests, ok). New tests (12):
  - `encoding::tests`: `unknown_quality_override_fails_resolution`,
    `unknown_speed_override_fails_resolution`,
    `malformed_audio_override_fails_resolution`,
    `mismatched_authority_combinations_fail_resolution`,
    `invalid_baseline_fails_resolution`,
    `manual_crf_without_display_name_derives_band`,
    `metadata_bands_cover_full_slider_and_match_derivation`,
    `preview_uses_production_resolution_and_warns_on_inapplicable_controls`
  - `types::tests`:
    `render_boundary_resolves_baseline_plus_overrides_and_derives_intent`,
    `render_boundary_without_overrides_keeps_baseline_and_passthrough`,
    `render_boundary_rejects_invalid_overrides_before_any_job`,
    `legacy_output_job_without_overrides_still_deserializes`
  - All 17 pre-existing `encoding::tests` adapted to `Result` (via
    `must_resolve` helper) and still pass unmodified in intent.
- `npx tsc --noEmit` → clean, no output.
- `cargo clippy --all-targets` → 30 warnings, all pre-existing in untouched
  areas (`auth/`, `subtitles/`, pre-existing `video/` lints such as
  `too_many_arguments` on old functions); **none** reference new code except
  two `doc_lazy_continuation` notes on pre-existing doc text.
- `npx vite build` → success (`✓ built in 2.93s`, 72 modules).
- Manual checks performed: `git diff --stat` confirms preset JSONs,
  `ffmpeg_args_builder.rs`, `ffmpeg.rs`, and the passthrough predicate are
  untouched; `git status` confirms the file inventory in §H.

### Not verified

```text
Not verified: live Tauri IPC round-trip (get_encoding_metadata /
  resolve_encoding_preview through the invoke layer) and visual UI states.
Reason: no running-app harness exists in this environment.
Impact: low — command logic is unit-tested; registration verified statically
  (lib.rs handler + capabilities allowlist + specta export); tsc + vite build
  prove the frontend contract compiles and bundles.
```

```text
Not verified: output-level FFmpeg application of settings.
Reason: explicitly out of scope per Issue #3; no harness exists.
Impact: none for this patch (no encoder behavior changed).
```

Never implied: no claim is made above beyond the verification actually run.

---

## G. Regression Check

**Existing behavior verified (all passing, unmodified in intent):**

- All 17 pre-existing `encoding::tests` (resolution semantics, authority
  switching, stale-CRF discard, full-slider bands, passthrough predicate,
  arg-level `-crf/-preset/-b:a`, `-an` omission).
- All 11 `ffmpeg_args_builder::tests` (codec branches, `-threads`, `-pix_fmt`,
  `-movflags`, ASS chaining) — builder byte-identical.
- All 6 `validation::tests`, scheduler/concurrency/lock/text-fonts suites
  (only touch-point: scheduler fixture gained the new required field with
  `Baseline` semantics — behavior-neutral).
- Legacy deserialization: `legacy_output_job_without_overrides_still_deserializes`
  proves old payloads (no `encodingOverrides`, no `forceReencode`) load and
  resolve to pure baseline.

**Areas inspected for regressions:**

- **Preset selection:** baseline flows by shared reference, never mutated
  (frontend `deepClone` on send; backend takes `&OutputJob` and clones).
- **Transient overrides:** preserved across preset switches (preview effect
  re-resolves new baseline + same overrides); cleared only by reset.
- **Custom presets:** save path unchanged apart from comment; load path
  unchanged (`load_custom_presets`, DTO mapping untouched).
- **Passthrough:** predicate untouched; `force_reencode=false` baseline jobs
  retain copy eligibility; any override forces re-encode (predicate tests +
  constructor tests).
- **CRF / quality / speed / audio:** mapping tables and reps byte-identical
  (`every_quality_preset_has_a_representative_crf_in_order` passes);
  preset-specific baseline CRFs preserved.
- **Batch + single rendering:** both converge on `resolve_for_render`;
  subtitle/preview layout paths behavior-identical (`for_layout` carries the
  same field values, including real `input_path` for logo-aware subtitle
  measurement).
- **Frontend/backend contract:** `backend.ts` regenerated from specta
  (`cargo run --bin export_types`); `tsc` + `vite build` green.

**Potential regression risks (accepted, minor):**

1. Stricter intake: malformed/legacy payloads that previously rendered with
   silent fallbacks now fail the job with `InvalidInput`. This is the
   intended Issue #7 hardening; batch surfaces it as a per-job deterministic
   failure, single-render as a command error.
2. Preview is async: first paint uses fallbacks until `get_encoding_metadata`
   + first preview resolve; momentary display lag on selection change is
   possible (local IPC, millisecond scale).
3. `Baseline` + stray quality values now error instead of being ignored —
   only reachable via hand-crafted IPC, not via the shipped UI.

---

## H. Files Changed

```text
src-tauri/src/video/encoding.rs
    Change type: added (staged-new pre-existing) + heavily modified (unstaged)
    Purpose: serializable QualityAuthority/EncodingOverrides; shared validators;
      fallible resolve_effective_encoding; metadata + preview + 2 Tauri commands;
      8 new tests
    Related issue: #1, #2, #4, #5, #6, #7

src-tauri/src/video/types.rs
    Change type: modified
    Purpose: OutputJob.encoding_overrides (+force_reencode legacy docs);
      ResolvedJob::resolve_for_render + for_layout; 4 new tests
    Related issue: #1, #4, #7

src-tauri/src/video/validation.rs
    Change type: modified
    Purpose: delegate encoding checks to video::encoding; validate overrides
      in validate_output_job
    Related issue: #2, #7

src-tauri/src/video/mod.rs
    Change type: modified
    Purpose: convert_to_ratio via resolve_for_render; preview via for_layout
    Related issue: #1, #4

src-tauri/src/video/batch_processor.rs
    Change type: modified
    Purpose: subtitle layout via for_layout; render via resolve_for_render
      with deterministic-failure path
    Related issue: #1, #4, #7

src-tauri/src/video/convert.rs
    Change type: modified (additive logs only)
    Purpose: VP9-speed and remove-audio applicability diagnostics
    Related issue: #5, #6

src-tauri/src/video/scheduler.rs
    Change type: modified (1 line)
    Purpose: test fixture supplies encoding_overrides (Baseline)
    Related issue: #1 (test compat)

src-tauri/src/bin/export_types.rs
    Change type: modified
    Purpose: register 7 new specta types
    Related issue: #2

src-tauri/src/lib.rs
    Change type: modified (2 lines)
    Purpose: register get_encoding_metadata + resolve_encoding_preview
    Related issue: #2

src-tauri/capabilities/default.json
    Change type: modified (2 lines)
    Purpose: allow the 2 new commands
    Related issue: #2

src/types/backend.ts
    Change type: modified (regenerated via export_types)
    Purpose: EncodingOverrides, QualityAuthority, EncodingMetadata,
      QualityLevelMeta, CodecCapability, EncodingPreviewRequest/Response;
      OutputJob.encodingOverrides/forceReencode docs
    Related issue: #1, #2

src/App.tsx
    Change type: modified
    Purpose: delete duplicated rules (~120 lines); metadata-driven option
      lists; backend preview (debounced) for display; baseline+overrides
      render requests; VP9/remove-audio disabled states + warnings;
      snapshot comment
    Related issue: #1, #2, #4, #5, #6, #8

encoding_architecture_fix_audit.md
    Change type: added (this report)
    Purpose: required §18 deliverable
    Related issue: all
```

Not changed (deliberately): `platform_specific_presets.json`,
`aspect_ratio_presets.json`, `ffmpeg_args_builder.rs`, `ffmpeg.rs`,
passthrough predicate (`is_passthrough_allowed`), filter/scheduler/concurrency
logic, auth/subtitles/config subsystems.

Latest `git status --short` (read-only inspection; no mutation performed):

```text
AM encoding_architecture_fix_audit.md
M  src-tauri/capabilities/default.json
M  src-tauri/src/bin/export_types.rs
M  src-tauri/src/lib.rs
M  src-tauri/src/video/batch_processor.rs
M  src-tauri/src/video/convert.rs
A  src-tauri/src/video/encoding.rs
M  src-tauri/src/video/mod.rs
M  src-tauri/src/video/scheduler.rs
M  src-tauri/src/video/types.rs
M  src-tauri/src/video/validation.rs
M  src/App.tsx
M  src/types/backend.ts
```

Notes on repository state (Issue #9 compliance):

- I ran **no** mutating Git command in either work session — only read-only
  `status`, `diff`, `log --oneline`, and `stash list` (empty; `HEAD` remains
  `1157721`, no new commits). Index contents are environment-managed; an
  earlier `status` read rendered the same tree with unstaged (`MM`/`??`)
  markers, while the latest read shows the tree staged. The distinction does
  not affect the patch: worktree content was re-verified intact after the
  final report write (all patch symbols — `resolve_for_render`, `for_layout`,
  `resolve_encoding_preview`, `encoding_overrides`, `scheduleEncodingPreview`
  — present in the working files; full `cargo test` suite green on this
  exact tree).
- Net effect vs `HEAD 1157721`: the 12 source files above plus this report
  carry the complete change (pre-existing staged encoding model + this
  task's hardening patch); preset JSONs, FFmpeg pipeline, and passthrough
  predicate are absent from the diff.

---

## I. Remaining Concerns

### Fully fixed (verified as resolved)

- #1 Rust render-boundary authority (constructor-level proof).
- #2 Duplicated business logic (deletion + metadata/preview proof).
- #4 Derived re-encode intent (both-direction proof; layout separation).
- #5 VP9 speed applicability (metadata + warning + disabled UI + log).
- #6 Remove-audio bitrate applicability (warning + disabled UI + preserved
  selection + log).
- #7 Fallible resolution boundary (5 intent tests + boundary rejection test
  + deterministic batch failure path).

### Partially fixed

- #2 live-contract verification: implementation complete; Tauri invoke-layer
  round-trip and visual states verified statically only (no harness).
  Non-blocker: logic unit-tested, contract type-checked end to end
  (`specta` → `backend.ts` → `tsc`), commands registered in both required
  places.

### Unresolved

- None within task scope. #3 required no change by design; #8 was confirmed
  not-a-bug; #9 forbade the only action (Git ops) by design.

For completeness, pre-existing gaps this patch does **not** claim to close
(all outside the task's scope): automated output-level encode tests,
frontend unit-test runner, and the unrelated `compute_preview_layout` /
config commands missing from the capabilities allowlist (observed while
registering the new commands; left untouched per minimal-change Rule 6 —
flagged here for a follow-up).

---

## J. Final Assessment

| Issue | Status | Evidence |
| ----- | ------ | -------- |
| #1 | Fully fixed | Both render paths call `resolve_for_render`; `render_boundary_*` tests; 204/204 suite |
| #2 | Fully fixed | Duplicates deleted (grep-clean); metadata/preview commands + tests; `tsc` + `vite build` green (live round-trip statically verified — see §F) |
| #3 | Fully fixed | No code change per instruction; gap-vs-failure distinction documented; diagnostics added |
| #4 | Fully fixed | Intake ignored both directions (tests); `for_layout` separation; no remaining render literals |
| #5 | Fully fixed | Capability metadata + preview warning + disabled UI + render log; builder untouched |
| #6 | Fully fixed | Disabled UI + preserved selection + preview warning + render log; `-an` untouched |
| #7 | Fully fixed | `Result` resolver; 5 intent tests + boundary rejection + batch failure path |
| #8 | Fully fixed | Confirmed snapshot semantics; no redesign; save-path comment |
| #9 | Fully fixed | Zero Git mutations; state table in §H |

No issue is marked "Fully fixed" without supporting implementation +
verification above. The §21 Definition of Done is satisfied item-by-item:
architecture inspected before modification (§B/C roots); every issue checked
against implementation (§B); root causes identified (§C); Rust authoritative
(§D1/F-#1); no frontend-owned resolution (§D2/F-#2); derived intent (§D1/F-#4);
invalid states fail pre-render (§D3/F-#7); codec applicability explicit
(§D4/F-#5/#6); semantics intact (§G); snapshots intact (§D5/G); FFmpeg
pipeline untouched (§B-#3/H); tests/checks/builds run (§F); regressions
checked (§G); no unrelated refactors (§H "Not changed"); no Git mutations
(§B-#9/H); docs synchronized (field/command comments + this report); audit
report created (this file); every issue has a status (§J); deviations
documented (§A/§D).

Final principle check: the implementation makes the encoding system harder to
misuse (invalid intent cannot construct a job; intent cannot be faked via
`forceReencode`; two codebases cannot drift) while preserving the verified
working behavior (identical tables, identical builder, identical passthrough
geometry, identical preset values).
