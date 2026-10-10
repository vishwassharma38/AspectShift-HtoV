# Text Overlay Parity Report — editor preview vs exported video

Branch: `fix/text-overlay-parity` (off `experimental`).
Harness: `scripts/measure-text-parity.py` (manual live render).
Date: 2026-09-30. FFmpeg 8.1.2-full_build-www.gyan.dev (libass via `ass`
filter), Chrome headless DOM reference, PIL pixel-scan measurement.

---

## 1. Diagnosis (my own opinion, formed before leaning on the hypothesis)

**Screenshot mapping correction:** the prompt's table has the two images
swapped. Viewed directly:

- `text_overlay_bug_1.PNG` = **editor preview** (app UI, aspect-ratio
  targets, GUIDES/SAFE AREAS, Twitter/X 16:9 1280x720 selected). Text
  "REVENANT GO BRRRRR" is huge and blocky, ~96% of frame width.
- `text_overlay_bug_1_2.PNG` = **exported frame** (plain frame, no UI).
  Same text, same top-center position, ~37% of frame width.

Preview is ~2.6x wider than export. Position looks correct; **size** is the
problem. Font is Retro/Bungee by layer settings and glyph shapes (verified).

**Ruled out (verified by reading + render):**

- Geometry/scale math (`overlayGeometry.ts`, `toPreviewFontSize`,
  `toVideoPosition`, signed `\pos`, off-canvas lifecycle): correct. The
  Phase 1–6 scripts pass because they compare the app's own numbers with
  each other — the bug is downstream, in what libass *does* with those
  numbers.
- `PlayRes` vs output resolution: `prepare_text_overlay()` sets
  `play_res = target_width/height` and the `ass` filter is appended **after**
  the scale-to-target stage, so PlayRes == video frame 1:1. No double scale.
- Filter-graph order: scale/crop first, `ass` last. Correct.
- Font fallback/`fontsdir`: `ass_name` values match the bundled families'
  `nameID 1` (Bungee, Fira Sans, …); all 4 faces per family exist as distinct
  files; bold/italic probe renders distinct widths (e.g. Bungee bold 234px
  vs regular 223px at FS 48), so `Bold=-1`/`Italic=-1` resolve to the real
  faces on both sides (preview `@font-face` weight/style mapping is also
  real). No fallback involved.
- Bold/italic synthesis mismatch: not a cause — both sides use real faces.

**Confirmed causes (verified by render, not just reading):**

1. **Primary: CSS em vs ASS win-cell.** Preview `font-size` sets the em
   square; libass scales so `usWinAscent + usWinDescent` = `Fontsize`.
   Measured export/preview at nominal 48 (FFmpeg `ass` + PIL/Chrome):
   Bungee 0.390 (theory 0.3885), Fira 0.822 (theory 0.833), Anton 0.576
   (theory 0.577) — i.e. the screenshot's 2.6x is Bungee's 2.574. libass
   uses **win** metrics, not hhea/typo (Bungee hhea ratio would give 0.76,
   measured 0.39).
2. **Letter spacing dropped.** Preview sets `letterSpacing` for minimal
   (0.04em) and gaming/cyberpunk (0.03em); the writer hardcoded ASS
   `Spacing = 0`. At 48px this is ~35px (~6%) of missing width for those
   three styles.
3. **Outline double-counted.** Preview `-webkit-text-stroke w` with
   `paint-order: stroke fill` shows ~w/2 outside; ASS `Outline` draws the
   full width outward. Export outlines render ~2x the preview's visible
   thickness (plus ~1px AA fringe measured with a red-outline probe).
4. **Auto-wrap mismatch.** Preview `white-space: pre` never auto-wraps;
   the text-overlay ASS had no `WrapStyle`, so libass smart-wrapped long
   lines (proven: 1330px single-line preview became 643x247 wrapped export
   at large sizes). Multi-line `\N` breaks work on both sides; only
   *automatic* wrapping differs.

## 2. Measurement table (post-fix, nominal 48, 1280x720)

Text `REVENANT GO BRRRRR`, white, no outline, `ass_font_size =
round(48 * win/UPM)`, `Spacing` per style, `WrapStyle: 2`. Preview = Chrome
headless DOM `offsetWidth` with identical `@font-face` + CSS
(`white-space: pre`, per-style `letterSpacing`). Export = FFmpeg `ass`
render ink bbox (threshold 30).

| Style (font) | win/UPM | ASS FS | ASS Sp | Preview px | Export px | Err % |
| --- | --- | --- | --- | --- | --- | --- |
| clean (Fira Sans) | 1.2000 | 58 | 0 | 488.0 | 487x35 | -0.20 |
| minimal (Lato) | 1.4160 | 68 | 1.92 | 565.0 | 560x36 | -0.88 |
| caption (Inter) | 1.4302 | 69 | 0 | 544.0 | 543x37 | -0.18 |
| meme (Anton) | 1.7334 | 83 | 0 | 381.0 | 381x42 | +0.00 |
| creator (Montserrat) | 1.5620 | 75 | 0 | 584.0 | 579x34 | -0.86 |
| gaming (Exo 2) | 1.4690 | 71 | 1.44 | 525.0 | 523x36 | -0.38 |
| cyberpunk (Orbitron) | 1.2540 | 60 | 1.44 | 687.0 | 679x36 | -1.16 |
| cinematic (Cormorant) | 1.3800 | 66 | 0 | 519.0 | 516x32 | -0.58 |
| retro (Bungee) | 2.5740 | 124 | 0 | 576.0 | 575x36 | -0.17 |
| handwritten (Caveat) | 1.2890 | 62 | 0 | 416.0 | 421x40 | +1.20 |

Heights (export ink vs Chrome `actualBoundingBox` ink): all within **±1px**
(clean 35/35, minimal 36/35, caption 37/37, meme 42/42, creator 34/35,
gaming 36/35, cyberpunk 36/35, cinematic 32/33, retro 36/37, handwritten
40/40). 1px at ~35px glyph height is AA quantization, not a geometry error.

Also verified: size 64 at 1920x1080 ALL PASS (worst -1.20%); 1080x1920,
1080x1080, 1080x1350 identical absolute px (fontSize is absolute video px,
not width-relative) ALL PASS. Pre-fix errors were -61% (Bungee) to -18%
(Fira); spacing-only errors were -6.8% (minimal), -6.0% (cyberpunk), -4.4%
(gaming).

Reproduce: `python scripts/measure-text-parity.py [--size N]
[--width W] [--height H] [--text "..."] [--out dir]`.

## 3. Agree / disagree with the proposed hypothesis

**Agree (primary).** The em-vs-win-cell hypothesis is correct and complete
as the size cause: my TTF dump reproduces the prompt's table to 3 decimals
(Bungee 2.574/0.3885, Anton 1.7334/0.5769, …), and my libass renders match
`UPM/win` to <1.5% for every style. The screenshot ratio (~2.6x) is
Bungee's 2.574. The Phase 1–6 tests passed because they check internal math,
never rendered pixels — exactly as the prompt suspected.

**Agree (secondaries 1–2), with a direction chosen:** outline half-visibility
and dropped letter spacing are both real; I fixed both on the export side
(see §4). **Partly agree (secondary 3):** multi-line line-spacing mismatch
is real but I did **not** fix it (see §6) — preview `lineHeight`
(0.95/1.05/1.15 × em) vs libass line pitch (≈ ASS `Fontsize`, i.e. the win
cell). Measured 2-line minus 1-line bbox: retro 124px vs preview 45.6px
(2.7x), meme ~1.8x, handwritten +23%, clean +4%. **Agree (secondary 4
candidates):** bold/italic faces, fallback, `fontsdir`, `ScaledBorderAndShadow`,
PlayRes, filter order were all checked and are **not** causes (see §1).

## 4. What I changed and why (export matches preview)

Preview is what the user positions against, and canonical `fontSize`
storage keeps its em meaning (no migration). All changes are export-side:

- `src-tauri/src/video/text_fonts.rs`: per-font `ass_cell_ratio`
  (win/UPM table) + `ass_font_size_for_style()` (nominal × ratio, rounded;
  integer rounding ≤1.1% at preset sizes 40+) + `ass_letter_spacing_for_style()`
  (0.04/0.03/0 mirroring `VideoCanvas.tsx`). Hardcoded table chosen over
  runtime TTF parsing (deterministic, no render-time IO/failure, works for
  both text and subtitle paths without plumbing; a `ttf-parser`
  dev-dependency test fails the build if bundled fonts ever change) and over
  empirical fudge (would overfit one libass version/text; win/UPM is the
  principled unit conversion, residuals are shaping/AA noise).
- `src-tauri/src/video/convert.rs` (`prepare_text_overlay`): ASS `Fontsize`
  = corrected size; ASS `Outline` = `outline_width / 2` (preview stroke shows
  outer half); ASS `Spacing` = preview `letterSpacing`. Canonical storage
  untouched.
- `src-tauri/src/subtitles/ass_writer.rs`: new `AssStyle::spacing` column
  (was hardcoded 0); `WrapStyle: 2` in **text-overlay** output only
  (matches `white-space: pre`; subtitle path keeps smart wrapping);
  `format_ass_float` helper (whole numbers stay `0`/`3`, fractions keep ≤2
  decimals).
- `src-tauri/src/subtitles/positioning.rs` (`calculate_ass_style`): same
  win/UPM upscale for burned subtitles (which share the 10 fonts); preview
  `SubtitleLayoutMetrics.font_size` keeps em meaning, only the ASS value is
  corrected; subtitle `spacing` stays 0 (subtitle preview has no
  letterSpacing) and subtitle outline stays full (subtitle preview uses full
  `textShadow`, unlike the text-overlay stroke — see §6).

## 5. What I deliberately did not change

- Canonical `fontSize` semantics/storage, validation range (12..=240),
  frontend preview CSS/scale math, geometry helpers, signed `\pos`,
  off-canvas lifecycle, drag bounds, auto-subtitle margins/wrapping,
  `PlayRes` already equal to target, filter-graph order, `fontsdir`
  handling, bold/italic face selection (verified correct), SRT export.
- ASS `Fontsize` stays integer (fractional remains a possible follow-up for
  size-12 edge: 12×1.2=14.4→14 is 2.8%; typical presets 40+ stay ≤1.1%).
- Multi-line line pitch (documented below, not refactored — would require
  per-line Dialogues with explicit `\pos`, out of scope for a size fix).

## 6. Risks, tolerances, known limitations

- **Outline tolerance: ±1px at 720p** (AA fringe; red-outline probe shows
  ~1.5x fringe over nominal). Halving is directionally verified; pixel-exact
  stroke-vs-outline rasterization cannot match (different engines).
- **Height: ±1px** rather than strict 2% (1px/35px = 2.9% quantization;
  all styles within 1px, see §2).
- **Multi-line (known, unfixed):** export line pitch ≈ ASS `Fontsize`;
  preview pitch = nominal × 0.95/1.05/1.15. Worst: retro 2.7x, meme 1.8x,
  handwritten 1.23x, others 1.04–1.36x. Fix direction: split
  `write_text_overlays_ass` one-Dialogue-per-line with `\pos` offsets from
  preview `lineHeight` (needs a `line_height` plumbed from
  `getTextLayerStyle` factors). Single-line (the bug report) fully passes.
- **Bold/italic:** verified resolving to real faces with correct relative
  widths; same win ratio covers them (all 4 faces share identical win/UPM
  per family, dumped). Preview `fontSynthesis` never synthesizes because
  real faces exist.
- **Rounding:** integer ASS sizes; size-12 minimum edge noted above.
- **Existing suites:** `cargo test --lib` 224/224 (was 216; +8 new, 2
  validation tests untouched from Phase 4); all `scripts/verify-*.mjs`
  unchanged and passing; `verify-text-style-matrix.ps1` unaffected
  (font discovery unchanged); no clamps reintroduced, signed `\pos`
  preserved.
- **If fonts update:** `ass_cell_ratios_match_bundled_win_metrics` fails
  with the new win/UPM — re-measure and update the table (values + report).

## 7. Verification performed

- Rendered: every style × nominal/corrected × 1280x720/1920x1080/1080x1920/
  1080x1080/1080x1350, bold/italic face matrix, red-outline thickness probe,
  1-line vs 2-line pitch probe, `WrapStyle: 2` long-line no-wrap check —
  all via the harness + temp probes (Chrome DOM reference, FFmpeg `ass`
  with `fontsdir`, PIL bbox scan). Labeled above as verified-by-rendering;
  everything else (filter order, PlayRes equality, face discovery) is
  verified-by-reading plus the cited structural/behavioral suites.
- No live Tauri render was run (needs app runtime + media fixtures); the
  harness replicates `write_text_overlays_ass` + `with_ass_filter` exactly,
  including escaping, PlayRes, alignment, and fontsdir.
