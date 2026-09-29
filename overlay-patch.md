# Overlay Patch Roadmap

## 1. Problem / Bug Context

AspectShift-HtoV is experiencing overlay-related preview/output inconsistencies that are not isolated UI glitches. The identified bugs are symptoms of how overlay geometry is currently represented and translated between the editor preview and the final renderer.

The current conceptual model is:

> “The overlay has one meaning in the editor, and then we'll figure out what that means again when rendering.”

The desired model is:

> “The overlay has one meaning. The editor shows that meaning, and the renderer renders that exact same meaning.”

This is the architectural change driving the roadmap.

### Example of the current geometry model

For a video such as:

```text
1080 × 1920
```

an overlay may be stored roughly as:

```text
x = 0.5
y = 0.5
fontSize = 48
```

The intended meaning is:

- `x = 0.5` → center of the video.
- `y = 0.5` → center of the video.
- `fontSize = 48` → roughly 48 video pixels of font size.

A smaller preview, such as:

```text
270 × 480
```

is exactly 25% of the actual video's size, so a preview can mathematically represent the stored font size as:

```text
48 × 0.25 = 12px
```

The renderer can continue to receive:

```text
fontSize = 48
```

and render at the video's actual resolution.

In theory, these representations should remain proportional. The identified problems begin when the preview performs additional transformations or mutations that the renderer does not perform.

### Core preview/output mismatch

The preview uses:

**HTML/CSS browser text rendering.**

The final output uses:

**FFmpeg → libass text rendering.**

The two systems do not have identical text-rendering behavior. The preview uses browser behavior such as:

```text
lineHeight
letterSpacing
-webkit-text-stroke
fontSynthesis
CSS wrapping
```

while output rendering uses:

```text
libass
Spacing = 0
ASS outline
real font files
different text metrics
```

Therefore, even when both systems receive:

```text
fontSize = 48
```

they are not guaranteed to produce pixel-identical text boxes. This explains a substantial part of the observed behavior where text appears one size in the editor and another size in the output.

### Preview minimum-font-size issue

The preview uses:

```text
Math.max(8, scaledFontSize)
```

This means the mathematically correct preview size can be replaced by an arbitrary minimum.

For example, if the correct preview size is:

```text
4px
```

the preview displays:

```text
8px
```

which is twice the mathematically correct size.

Likewise, if the correct preview size is:

```text
3px
```

the preview still displays:

```text
8px
```

This can make the preview dramatically larger than the final output, especially when the preview window size changes.

A key requirement is therefore:

> **Changing the editor window should not change the logical appearance of the overlay.**

### Preview position mutation issue

The preview contains a mechanism named:

```text
clampTextToFrame
```

This mechanism measures the HTML text box and moves the overlay back inside the frame when it approaches an edge. Critically, it changes the stored `x/y` position rather than only changing the temporary visual representation.

For example, a user may intentionally set:

```text
x = 0.05
```

or otherwise place text mostly outside the left side. The preview can inspect its HTML bounding box and modify the actual stored position to force the text back inside.

This is not a purely visual adjustment. It changes the user's actual overlay data.

### Preview wrapping issue

The preview also applies:

```css
max-width: 100%;
overflow-wrap: anywhere;
```

This means the browser is told not to let the text become wider than the video container.

For example:

```text
I love this
```

can become:

```text
I love
this
```

in the preview even when the final renderer produces:

```text
I love this
```

This is another direct source of preview/output mismatch.

### Geometry versus visibility

The intended model distinguishes between:

**Geometry**

> Where does the object exist?

and

**Visibility**

> Which part of that object happens to be visible inside the video?

The current system mixes these concepts by treating the video frame as a geometry boundary rather than a clipping boundary.

The desired conceptual behavior is:

```text
       OVERLAY
 ┌───────────────────────┐
 │     HELLO WORLD       │
 └───────────────────────┘
          ↓
┌─────────────────────────┐
│     VIDEO FRAME         │
│     HELLO WOR           │
└─────────────────────────┘
```

The overlay exists independently. The video frame is the viewport/clipping boundary.

### Backend position clamping issue

The backend currently performs:

```rust
x.clamp(0.0, 1.0)
```

which prevents values outside the range from surviving.

For the intended behavior, values such as:

```text
x = -0.1
```

are valid and mean that the center of the overlay is 10% of the video width to the left of the frame.

Likewise:

```text
x = 1.1
```

means that the center is 10% beyond the right edge.

The renderer should render the overlay and let the video boundary clip the invisible part.

The same conceptual rule applies to both frontend and backend:

**the frame is a clipping boundary, not an overlay geometry boundary.**

---

## 2. Key Findings

### 2.1 The fundamental issue is architectural

The identified bugs are symptoms of inconsistent overlay geometry semantics between the editor and renderer rather than isolated UI glitches.

The current model effectively has:

```text
One stored value
      ↓
Preview interprets it one way
      ↓
Preview modifies/clamps it
      ↓
Renderer interprets it another way
```

The target model is:

```text
One canonical overlay state
          ↓
    ┌─────┴─────┐
    ↓           ↓
 Preview      Renderer
    ↓           ↓
 Screen       Video
```

### 2.2 Preview and final output use different text engines

Preview rendering is based on HTML/CSS browser text rendering, while final output uses FFmpeg/libass.

The engines can differ in:

- font metrics
- line height
- letter spacing
- font synthesis
- wrapping
- outline behavior
- spacing
- positioning
- actual font files

Therefore, geometric parity can be targeted strongly, but pixel-identical typography cannot be assumed.

### 2.3 The preview applies an arbitrary minimum font size

The preview uses:

```text
Math.max(8, scaledFontSize)
```

which can make the preview render a text size larger than the mathematically correct scaled size.

This is a direct cause of preview/output size drift.

### 2.4 The preview mutates canonical position

`clampTextToFrame` is not just a visual constraint. It can modify stored `x/y` values based on the measured HTML text box.

This makes the preview an active modifier of project state rather than a pure representation of state.

### 2.5 The preview wraps freeform text through viewport constraints

The combination of:

```css
max-width: 100%;
overflow-wrap: anywhere;
```

causes wrapping to occur as a consequence of the preview container rather than as an explicit overlay behavior.

### 2.6 The video frame is currently treated as a geometry boundary

Both preview clamping and backend clamping limit overlay geometry to the visible video rectangle.

The desired behavior is for overlay geometry to exist independently and for the video frame to determine only what is visible.

### 2.7 Normalized `x/y` coordinates are appropriate

The response recommends keeping `x/y` normalized because AspectShift renders the same project to different target ratios and resolutions.

For example:

```text
x = 0.75
y = 0.50
```

means:

> 75% across the video, 50% down.

That meaning remains valid for both:

```text
1920 × 1080
```

and:

```text
1080 × 1920
```

By contrast, absolute coordinates such as:

```text
x = 1440
y = 540
```

would need reinterpretation whenever the target resolution changes.

### 2.8 Normalized position coordinates should not be limited to `0..1`

The desired position semantics are that `x` and `y` may be any finite numeric values, including values outside `[0,1]`.

Values such as:

```text
-0.2
0
0.5
1
1.2
```

are legitimate overlay positions.

### 2.9 `fontSize` normalization is a larger and separate migration decision

The response explicitly cautions against combining the immediate bug fix with a migration from:

```text
fontSize
```

to something such as:

```text
fontScale
```

normalized relative to video height.

The existing semantics:

```text
fontSize = 48 video pixels
```

are understandable and should be preserved during the initial bug fix.

Changing the stored meaning to a normalized value such as:

```text
fontScale = 0.025
```

would be a larger change. Existing overlays could change size across resolutions.

Therefore, the immediate fix should preserve the existing persisted `fontSize` semantics. Typography normalization can be evaluated separately later if there is a concrete product requirement for resolution-relative typography.

### 2.10 Explicit text wrapping is a long-term design opportunity

The response identifies accidental wrapping as a larger product-design opportunity.

A future model could distinguish:

```text
Text Overlay

Wrapping:
  ○ None
  ○ Fixed width
```

Under no wrapping:

```text
I love this gigantic sentence
```

would extend naturally and clip at the frame.

Under fixed width:

```text
I love this
gigantic
sentence
```

would wrap intentionally.

The width would become part of the canonical overlay model, so the preview and renderer would both operate on an explicit text-box width rather than allowing the preview viewport to determine wrapping implicitly.

### 2.11 Subtitle behavior is a separate product concept

The response explicitly distinguishes automatic subtitles from freeform overlays.

Freeform text overlays should use:

```text
unbounded geometry
no automatic wrapping
frame clips it
```

Manual subtitles may potentially use the same behavior.

Automatic subtitles can instead use:

```text
defined safe/margin region
intentional wrapping
```

The distinction is important because the requirement to prevent accidental wrapping for freeform overlays should not become a blanket rule that changes automatic subtitle behavior.

### 2.12 Browser/libass parity has a practical limit

The target should be strong geometric parity in:

- position
- size semantics
- clipping
- scale
- alignment

The response does not recommend requiring pixel-identical typography because:

```text
Chrome text engine ≠ libass
```

A suitable product invariant is:

> The preview is a faithful representation of the overlay's logical geometry and intended styling; minor rasterization/text-metric differences between preview and final renderer are acceptable.

### 2.13 Window resizing must not change logical overlay state

Resizing the preview from one screen size to another must not modify:

```text
x
y
fontSize
scale
```

Only the screen representation should change.

### 2.14 Off-canvas positions must survive persistence

A deliberately positioned overlay such as:

```text
x = -0.1
```

must remain:

```text
x = -0.1
```

after save/reload.

It must not become:

```text
0
```

or be moved back inside by preview logic.

### 2.15 Preview and output should share the same geometry semantics

For a position such as:

```text
x = 0.75
```

the corresponding normalized position should satisfy approximately:

```text
previewX / previewWidth ≈ 0.75
```

and:

```text
outputX / outputWidth = 0.75
```

This is a useful invariant for regression testing.

### 2.16 A golden visual regression suite is a useful future test mechanism

The response recommends eventually adding a golden visual regression suite for this class of bug.

A representative case described in the response is:

```text
1080×1920 video
text = "I love this"
x = -0.15
y = 0.50
fontSize = 96
```

The suite can capture:

```text
preview screenshot
```

and:

```text
rendered output frame
```

and compare them using a reasonable tolerance rather than requiring strict pixel-for-pixel equality.

The proposed comparison dimensions are:

- approximate bounding box
- center position
- visible clipping boundaries
- text dimensions
- major styling properties

This is intended to catch this regression class before it is discovered manually.

---

## 3. ChatGPT Recommended Solution — Summary

The recommended solution is to move AspectShift toward a canonical overlay-geometry model in which a single overlay state is interpreted by both preview and renderer without mutating or reinterpreting its meaning.

The target conceptual architecture is:

```text
                 PROJECT STATE
                      │
                      ▼
            ┌──────────────────┐
            │ Canonical Overlay│
            │     Geometry     │
            └────────┬─────────┘
                     │
          ┌──────────┴──────────┐
          │                     │
          ▼                     ▼
      PREVIEW                RENDERER
          │                     │
          ▼                     ▼
     CSS coordinates        Video coordinates
          │                     │
          ▼                     ▼
     Browser renderer        FFmpeg/libass
          │                     │
          └──────────┬──────────┘
                     ▼
                VIDEO FRAME
                     │
                     ▼
               CLIPPING ONLY
```

The recommended implementation priorities are:

1. **Stop clamping/mutating overlay geometry.**
2. **Allow overlays to extend outside the video frame.**
3. **Make the video frame the clipping boundary rather than the overlay-size boundary.**
4. **Make preview and output pure transformations of the same canonical geometry.**

The immediate implementation should preserve the existing persisted `fontSize` semantics and focus first on Phases 0–4.

The response specifically instructs that the implementation agent should not blindly implement the entire audit at once. It should first implement Phases 0–4, preserving the existing persisted `fontSize` semantics, without migrating `fontSize` to normalized `fontScale`.

It should also avoid altering unrelated automatic subtitle layout behavior during the initial fix.

The initial implementation should establish canonical geometry, remove geometry mutation and clamping, make freeform overlays overflow and clip correctly, and add regression tests.

Before modifying the code, the implementation agent should inspect the current implementation and produce a concise implementation plan identifying every affected file.

Typography normalization should then be evaluated separately as an independent engineering decision.

---

## 4. Implementation Agent Rules

The implementation agent must use the roadmap as an implementation-guided bug-fix/refactoring plan rather than blindly applying every sentence as a literal code change.

1. **Inspect before modifying.**
   - Inspect the current implementation first.
   - Identify every affected file and existing overlay-related test before changing behavior.
   - Produce a concise implementation plan identifying every affected file before modifying the code.

2. **Confirm the current behavior before removing or changing it.**
   - Do not blindly remove a named helper or every occurrence of a named pattern.
   - For `clampTextToFrame`, confirm whether the identified state-mutating frame-containment behavior is still present and whether the helper has other legitimate responsibilities. If it does, preserve or refactor those responsibilities rather than deleting the helper blindly.
   - For `x.clamp(0.0, 1.0)` / `y.clamp(0.0, 1.0)`, determine which clamp is overlay geometry and which clamp is legitimate validation or UI behavior. Do not perform a global search-and-replace.

3. **Preserve existing anchor and alignment semantics.**
   - Do not change the existing meaning of `x`, `y`, or overlay alignment/anchor behavior during this patch unless the implementation audit proves that those semantics themselves are inconsistent.
   - If `x/y` currently represent the center of the text box, a top-left corner, or another alignment anchor, preserve that existing meaning while removing the frame-containment bug.

4. **Limit the scope to freeform overlay geometry.**
   - For this patch, “freeform overlay” means manually positioned text/image overlays that use the editor's overlay positioning model.
   - Do not apply these geometry changes to automatic subtitle layout, subtitle-safe-area calculations, subtitle line breaking, or unrelated video/canvas layout unless the audit demonstrates that they share the same broken geometry path.

5. **Do not make unrelated migrations.**
   - Do not migrate `fontSize` to `fontScale` during the initial geometry fix.
   - Do not alter automatic subtitle behavior as collateral damage.
   - Do not expand the patch into a broader redesign of AspectShift's overlay system.

6. **Use finite position values.**
   - `x` and `y` may be any finite numeric values, including values outside `[0,1]`.
   - The intended model does not imply that `NaN`, `Infinity`, or `-Infinity` are valid overlay positions.

7. **Validate rather than silently inventing behavior.**
   - If implementation findings contradict the roadmap, stop and report the conflict rather than inventing a new architecture or silently expanding the scope.

8. **Preserve and strengthen testing.**
   - Identify existing overlay-related tests before changing behavior and preserve unrelated behavior.
   - Add or modify regression tests before or alongside implementation where practical.
   - Do not delete or weaken tests merely because they conflict with the new intended geometry semantics; determine whether the test encodes obsolete behavior or a legitimate invariant.

9. **Keep the initial phase gate explicit.**
   - Phases 0–4 must be implemented and validated before Phase 5 begins.
   - If Phases 0–4 reveal that the existing `fontSize` behavior cannot be fixed without changing the persisted representation, stop and report the conflict instead of silently performing a schema or semantic migration.

## 5. Detailed Implementation Roadmap

### Phase 0 — Establish Invariants

#### Objective

Establish the geometry and rendering invariants that the implementation must preserve while fixing the overlay bugs.

#### Required Changes

Document and protect the following invariants:

```text
x/y describe logical video-space position.

Preview size must not modify x/y.

Preview window size must not modify logical overlay state.

Video frame clips overlays.

Overlay geometry is not constrained by frame boundaries.

Preview and renderer consume the same canonical geometry.

Text wrapping is explicit behavior, not an accidental consequence of the viewport.
```

#### Expected Outcome

The implementation work has an explicit set of behavioral invariants that define the intended overlay model and can be used to protect against regressions.

#### OpenCode/Muse Spark 1.3 Implementation Audit

> **Status:**
> **Implementation Summary:**
> **Files/Areas Changed:**
> **Issues Encountered:**
> **Validation Performed:**
> **Notes:**

---

### Phase 1 — Centralize Geometry

#### Objective

Establish one obvious place where overlay coordinate semantics live so the preview and renderer interpret the same canonical geometry.

#### Required Changes

Introduce a minimal canonical geometry representation or utility layer appropriate to the existing architecture. The goal is centralized semantics and removal of duplicated coordinate calculations; do not create abstractions merely for naming purposes.

A representation conceptually like:

```text
CanonicalOverlayGeometry
```

may be used if appropriate, with corresponding frontend utilities.

The canonical geometry should make values such as:

```text
canonical.x
canonical.y
```

the single source of truth.

Introduce conceptual transformations such as:

```text
toPreviewPosition()
toVideoPosition()
```

instead of scattering independent calculations such as:

```text
x * canvasWidth
x * targetWidth
x * 100
...
```

Normalized `x/y` coordinates should remain the chosen coordinate representation. Their meaning is relative to the video, not to the preview viewport.

Preserve the existing overlay anchor/alignment semantics. Do not change whether `x/y` currently describe the center of the text box, the top-left corner, or another alignment anchor unless the implementation audit proves that the existing semantics themselves are inconsistent.

The normalized position model should permit finite values outside `0..1`, including examples such as:

```text
-0.2
0
0.5
1
1.2
```

subject to sensible finite-number validation.

#### Expected Outcome

There is one canonical overlay geometry model, with explicit transformations for preview and video coordinate systems.

The same overlay meaning can be represented in both the editor and final renderer without scattered or duplicated coordinate semantics.

#### OpenCode/Muse Spark 1.3 Implementation Audit

> **Status:** Implemented (Phase 1 only; Phases 2–6 not started)
> **Implementation Summary:** Introduced canonical video-space normalized
> position semantics with shared transform helpers on both sides
> (`toPreviewPercent`/`toPreviewPosition`/`toVideoPosition`/
> `previewDeltaToCanonical` in `src/utils/overlayGeometry.ts`;
> `to_video_x`/`to_video_y`/`to_video_position`/
> `overlay_center_*_expression` in
> `src-tauri/src/video/overlay_geometry.rs`). Preview (text, manual logo,
> manual subtitle) and renderer (ASS text/subtitle positions, logo overlay
> expression) now derive coordinates through this layer. All legacy clamps,
> wrapping, typography, and validation behavior preserved bit-for-bit.
> **Files/Areas Changed:** `src/utils/overlayGeometry.ts` (new),
> `src-tauri/src/video/overlay_geometry.rs` (new, 7 unit tests),
> `src/components/VideoCanvas.tsx`,
> `src-tauri/src/subtitles/ass_writer.rs`,
> `src-tauri/src/video/filter_builder.rs`,
> `src-tauri/src/video/mod.rs`,
> `scripts/verify-phase1-geometry.mjs` (new).
> **Issues Encountered:** Render-boundary `0..1` enforcement also lives in
> `validation.rs` (`logo.x/y`, `textOverlay.layers[].x/y`,
> `subtitleOverlay.x/y` range checks) in addition to the `.clamp(0.0, 1.0)`
> sites — Phase 4 must relax both together; left untouched per scope.
> **Validation Performed:** `cargo test --lib` 211/211 pass (204 existing +
> 7 new); `node scripts/verify-phase1-geometry.mjs` passes; `tsc --noEmit`
> clean for all touched files (2 pre-existing `App.tsx` timer-typing errors
> unrelated to this change); `cargo fmt --check` clean for all touched
> files (remaining diffs pre-existing in `encoding.rs`/`validation.rs`).
> **Notes:** Anchor audit: center-anchoring confirmed consistent on both
> sides (preview `translate(-50%,-50%)`, ASS `\an5`, logo `overlay_w/2`;
> auto subtitles bottom-center on both sides) — preserved, no conflict.

---

### Phase 2 — Stop Mutating Geometry During Preview

#### Objective

Make the preview a pure representation of overlay state rather than a mechanism that modifies stored overlay geometry.

#### Required Changes

Remove:

```text
clampTextToFrame
```

as a state-mutating mechanism.

The preview should conceptually follow:

```text
state
 ↓
render(state)
 ↓
pixels
```

and not:

```text
state
 ↓
render
 ↓
"hmm that's outside"
 ↓
modify state
 ↓
render again
```

The preview must not change persisted or canonical `x/y` values based on the HTML text bounding box.

An intentionally off-canvas position such as:

```text
x = -0.1
```

must remain unchanged when the preview is displayed, when the preview window is resized, and after save/reload.

#### Expected Outcome

Preview rendering becomes a pure representation of canonical state.

The preview no longer silently changes the user's actual overlay position.

Off-canvas overlay positions remain valid and stable.

#### OpenCode/Muse Spark 1.3 Implementation Audit

> **Status:**
> **Implementation Summary:**
> **Files/Areas Changed:**
> **Issues Encountered:**
> **Validation Performed:**
> **Notes:**

---

### Phase 3 — Remove Accidental Constraints

#### Objective

Allow freeform text overlays to exist independently of the visible video frame and prevent the preview viewport from accidentally determining text geometry or wrapping.

#### Required Changes

For this patch, “freeform overlay” means manually positioned text/image overlays that use the editor's overlay positioning model.

For normal freeform text, remove constraints equivalent to:

```css
max-width: 100%;
overflow-wrap: anywhere;
```

Use semantics closer to:

```css
white-space: pre;
```

while retaining:

```text
canvas overflow = hidden
```

The intended result is:

```text
TEXT
██████████████████████
       ↓
┌───────────────────────┐
│     VIDEO FRAME       │
│ █████████████         │
└───────────────────────┘
```

The text itself is not resized merely because it exceeds the frame. The frame clips the visible portion.

Do not turn this into a blanket “never wrap anything” rule for automatic subtitles. Automatic subtitles are a different product concept and can retain a defined safe/margin region with intentional wrapping.

For the long-term design, wrapping should become an explicit feature rather than an accidental consequence of the viewport. The potential model described in the response is:

```text
Text Overlay

Wrapping:
  ○ None
  ○ Fixed width
```

Under no wrapping, text extends naturally and clips at the frame. Under fixed width, wrapping happens intentionally.

The width should become part of the canonical overlay model so both preview and renderer know the intended text-box width instead of deriving wrapping from the preview window.

#### Expected Outcome

Freeform overlays can extend outside the video frame, do not automatically wrap because of the preview viewport, and are clipped by the video frame instead of being resized or repositioned to remain inside it.

Automatic subtitle layout behavior is not unintentionally changed by the freeform overlay fix.

#### OpenCode/Muse Spark 1.3 Implementation Audit

> **Status:**
> **Implementation Summary:**
> **Files/Areas Changed:**
> **Issues Encountered:**
> **Validation Performed:**
> **Notes:**

---

### Phase 4 — Remove Backend Clamping

#### Objective

Remove backend behavior that treats the video frame as a hard geometry boundary for free-positioned overlays.

#### Required Changes

Remove behavior equivalent to:

```rust
x.clamp(0.0, 1.0)
y.clamp(0.0, 1.0)
```

for free-positioned overlays, but only after confirming that the clamp is enforcing overlay geometry rather than serving a legitimate validation or UI purpose. Do not globally remove every `0.0..1.0` clamp.

The resulting `x/y` values may be any finite numeric values, including values outside `[0,1]`.

The renderer should instead use the canonical normalized position directly:

```text
videoX = x * videoWidth
videoY = y * videoHeight
```

For example:

```text
x = -0.2
```

should produce a negative video-space X value.

Likewise:

```text
x = 1.1
```

should position the overlay center beyond the right edge.

The renderer should render the overlay and let the video boundary clip the invisible portion.

This keeps geometry and visibility separate:

- geometry determines where the overlay exists;
- the frame determines which part is visible.

#### Expected Outcome

Overlay geometry is no longer forced into the `0..1` range by backend clamping.

Values outside the visible frame remain valid, persist through rendering, and are clipped only by the video frame.

#### OpenCode/Muse Spark 1.3 Implementation Audit

> **Status:**
> **Implementation Summary:**
> **Files/Areas Changed:**
> **Issues Encountered:**
> **Validation Performed:**
> **Notes:**

---

### Phase 5 — Fix Preview Scaling

#### Phase Gate

Phases 0–4 must be implemented and validated before Phase 5 begins. Do not modify persisted `fontSize` semantics during Phases 0–4. If those phases reveal that the existing font-size behavior cannot be fixed without changing the persisted representation, stop and report the conflict instead of silently performing a schema or semantic migration.

#### Objective

Ensure preview scaling is a mathematical transformation of the existing canonical font-size semantics rather than a presentation hack that changes the apparent geometry.

#### Required Changes

Remove geometry calculations that use arbitrary minimums such as:

```text
Math.max(8, ...)
Math.max(12, ...)
```

The preview should mathematically scale the existing canonical font size:

```text
previewFontSize =
    canonicalFontSize × previewScale
```

Do not replace this existing font-size model with normalized `fontScale` during this phase.

The response explicitly recommends preserving the existing persisted `fontSize` semantics during the initial bug fix.

If tiny text becomes difficult to interact with, that is described as a UI interaction problem rather than a geometry problem. The response suggests that this can be handled separately with selection handles/hitboxes without changing how the text itself is rendered.

#### Expected Outcome

Preview font size changes only as the canonical video-space font size is scaled into the preview coordinate system.

An arbitrary minimum no longer makes the preview text larger than the mathematically correct representation.

Changing preview window size changes only screen representation and not logical overlay state.

#### OpenCode/Muse Spark 1.3 Implementation Audit

> **Status:**
> **Implementation Summary:**
> **Files/Areas Changed:**
> **Issues Encountered:**
> **Validation Performed:**
> **Notes:**

---

### Phase 6 — Deal With Browser/libass Differences

#### Objective

Define and preserve realistic preview/output parity expectations while acknowledging that browser text rendering and libass text rendering are different engines.

#### Required Changes

Target strong geometric parity for:

- position
- size semantics
- clipping
- scale
- alignment

Do not treat pixel-identical typography as a required guarantee because:

```text
Chrome text engine ≠ libass
```

The intended product invariant is:

> The preview is a faithful representation of the overlay's logical geometry and intended styling; minor rasterization/text-metric differences between preview and final renderer are acceptable.

The initial bug fix should therefore focus on making the geometry coherent rather than introducing unexplained typography correction factors such as:

```text
fontSize × 0.937
```

or equivalent magic numbers.

#### Expected Outcome

Preview and output have strong geometric parity without treating browser/libass rasterization and text-metric differences as an architectural geometry failure.

The implementation has a clear boundary between exact logical geometry and unavoidable renderer-specific text differences.

#### OpenCode/Muse Spark 1.3 Implementation Audit

> **Status:**
> **Implementation Summary:**
> **Files/Areas Changed:**
> **Issues Encountered:**
> **Validation Performed:**
> **Notes:**

---

## 6. Validation and Regression Requirements

The response identifies explicit behavioral test cases and invariants that should be retained as regression tests.

### 6.1 Test A — Centered overlay

Use:

```text
x = 0.5
y = 0.5
fontSize = 48
```

Expected:

- preview is centered;
- output is centered; and
- resizing the preview does not change the stored state.

### 6.2 Test B — Negative X

Use:

```text
x = -0.1
y = 0.5
```

Expected:

- the value survives save/load;
- the preview does not modify it;
- the renderer receives `-0.1`; and
- the visible portion is clipped by the frame.

### 6.3 Test C — X greater than 1

Use:

```text
x = 1.1
y = 0.5
```

Expected:

- the same persistence, preview, renderer, and clipping expectations as Test B apply.

### 6.4 Test D — Preview resizing

Use the same overlay with two preview sizes, for example:

```text
preview A = 800px
preview B = 1200px
```

Expected:

```text
canonical x/y/fontSize unchanged
```

More generally, resizing the preview must not change:

```text
x
y
fontSize
scale
```

Only the screen representation should change.

### 6.5 Test E — Long freeform text

Use text deliberately longer than the viewport.

Expected:

- the freeform overlay does not acquire accidental wrapping;
- the frame clips the visible portion; and
- the preview does not modify the overlay geometry.

### 6.6 Test F — Persistence of off-canvas geometry

Use:

```text
x = -0.15
y = 1.10
```

Save and reload.

Expected:

```text
x = -0.15
y = 1.10
```

The values must survive persistence unchanged.

### 6.7 Test G — Preview/output geometry parity

For:

```text
x = 0.75
```

the normalized preview and output positions should satisfy:

```text
previewX / previewWidth ≈ 0.75
```

and:

```text
outputX / outputWidth = 0.75
```

### 6.8 Existing overlay-related tests

Before changing behavior, identify existing overlay-related tests and preserve unrelated behavior. Add or modify regression tests before or alongside implementation where practical. Do not delete or weaken tests merely because they conflict with the new intended geometry semantics; determine whether the test encodes obsolete behavior or a legitimate invariant.

### 6.9 Golden visual regression suite

The response recommends eventually adding a golden visual regression suite for this class of bug.

Representative test case:

```text
1080×1920 video
text = "I love this"
x = -0.15
y = 0.50
fontSize = 96
```

Capture:

```text
preview screenshot
```

and:

```text
rendered output frame
```

Compare them using a reasonable tolerance rather than requiring pixel-for-pixel identity.

The comparison can focus on:

- approximate bounding box
- center position
- visible clipping boundaries
- text dimensions
- major styling properties

The purpose is to catch this regression class before it is discovered manually.

## 7. Immediate Implementation Scope and Constraints

The response recommends a deliberately limited initial implementation scope.

### Immediate implementation scope

Implement **Phases 0–4 first**, while:

- preserving the existing persisted `fontSize` semantics;
- not migrating font sizes to normalized `fontScale` yet;
- not altering unrelated subtitle auto-layout behavior;
- establishing canonical geometry;
- removing geometry mutation and clamping;
- making freeform overlays overflow and clip correctly; and
- adding regression tests.

### Required pre-change inspection

Before modifying the code, OpenCode/Muse Spark 1.3 should inspect the current implementation and produce a concise implementation plan identifying every affected file.

The implementation agent should confirm that the identified behavior is still present and determine whether each affected path applies to freeform overlays, subtitles, or both before changing it.

### Explicit separation of typography normalization

The response recommends evaluating typography normalization separately after the initial geometry fix and testing are complete.

The future question is whether AspectShift should intentionally change the meaning of stored typography from:

```text
fontSize = 48 video pixels
```

to a normalized concept such as:

```text
fontScale = 0.025
```

This should not be combined with the emergency preview/output parity fix.

---

## 8. Target Architecture

The desired final conceptual architecture is:

```text
                 PROJECT STATE
                      │
                      ▼
            ┌──────────────────┐
            │ Canonical Overlay│
            │     Geometry     │
            └────────┬─────────┘
                     │
          ┌──────────┴──────────┐
          │                     │
          ▼                     ▼
      PREVIEW                RENDERER
          │                     │
          ▼                     ▼
     CSS coordinates        Video coordinates
          │                     │
          ▼                     ▼
     Browser renderer        FFmpeg/libass
          │                     │
          └──────────┬──────────┘
                     ▼
                VIDEO FRAME
                     │
                     ▼
               CLIPPING ONLY
```

The central architectural rule is:

> **The video frame is a viewport, not a cage.**

Therefore an overlay may validly have positions such as:

```text
x = -0.5
x = 0
x = 0.5
x = 1
x = 1.5
```

Its geometry does not change. Only its visible intersection with the video frame changes.

---

## 9. Final Implementation / Audit Summary

The response's overall conclusion is that AspectShift is facing an editor/rendering architecture problem rather than merely a CSS bug.

### Current model

```text
One stored value
      ↓
Preview interprets it one way
      ↓
Preview modifies/clamps it
      ↓
Renderer interprets it another way
```

### Desired model

```text
One canonical overlay state
          ↓
    ┌─────┴─────┐
    ↓           ↓
 Preview      Renderer
    ↓           ↓
 Screen       Video
```

The immediate priority remains:

1. **Stop clamping/mutating overlay geometry.**
2. **Allow overlays to extend outside the video frame.**
3. **Make the video frame the clipping boundary rather than the overlay-size boundary.**
4. **Make preview and output pure transformations of the same canonical geometry.**

Typography normalization and pixel-level typography parity remain separate engineering decisions rather than part of the emergency geometry fix.

The intended milestone is a coherent rendering model in which the editor and final renderer operate from the same canonical overlay geometry.

---
