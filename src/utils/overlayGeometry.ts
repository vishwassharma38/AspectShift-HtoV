/**
 * Phase 1 — Canonical overlay geometry (video-space normalization).
 *
 * There is one source of truth for freeform overlay position semantics:
 *
 * ```text
 * canonical x / canonical y
 * ```
 *
 * - Normalized relative to the **video frame**, not the preview viewport.
 *   `x = 0` → left edge of the video, `0.5` → horizontal center,
 *   `1` → right edge; likewise `y = 0` → top, `1` → bottom.
 * - Any **finite** number is a valid canonical value, including values
 *   outside `[0, 1]` (e.g. `-0.2`, `1.2`). `NaN`/`Infinity` are never valid.
 * - Anchor is **center**: the position marks the center of the overlay box.
 *   Preview uses `translate(-50%, -50%)`; the renderer uses ASS `\an5` /
 *   `overlay_w/2`. Phase 1 preserves this; it does not redesign anchors.
 * - Automatic subtitles are a separate product concept (margin-box layout)
 *   and are intentionally out of scope for this module.
 *
 * Preview and renderer derive coordinates from the same meaning:
 *
 * ```text
 * previewPx = canonical * previewFrameSize
 * videoPx   = canonical * videoFrameSize
 * ```
 *
 * Phase 2 removed the automatic preview mutation (`clampTextToFrame`), so
 * rendering or resizing the preview no longer rewrites canonical `x/y`.
 * Phase 4 removed the geometry clamps along the state → normalization →
 * validation → renderer path, so finite off-canvas values survive as signed
 * video-space coordinates and the frame clips visibility. Remaining
 * boundaries (drag limits, unrelated numeric validation such as opacity /
 * font size / scale, automatic-subtitle margin layout) are interaction or
 * product policy, not geometry.
 */

export interface CanonicalOverlayPosition {
  readonly x: number;
  readonly y: number;
}

export interface PreviewFrameSize {
  readonly width: number;
  readonly height: number;
}

export interface VideoFrameSize {
  readonly width: number;
  readonly height: number;
}

export interface PreviewPoint {
  readonly xPx: number;
  readonly yPx: number;
}

export interface PreviewPercent {
  readonly xPercent: number;
  readonly yPercent: number;
}

export interface VideoPoint {
  readonly xPx: number;
  readonly yPx: number;
}

/** A canonical overlay coordinate is valid only when finite (no NaN/Infinity). */
export function isFiniteOverlayCoordinate(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** Both axes must be finite numbers; values outside `[0, 1]` are allowed. */
export function isFiniteOverlayPosition(
  position: CanonicalOverlayPosition | null | undefined,
): position is CanonicalOverlayPosition {
  return (
    !!position &&
    isFiniteOverlayCoordinate(position.x) &&
    isFiniteOverlayCoordinate(position.y)
  );
}

/**
 * Canonical → preview pixels. Pure scale; no clamping, no mutation.
 * `previewFrameSize` is the on-screen video frame (canvas box), so resizing
 * the window changes only this output, never the canonical input.
 */
export function toPreviewPosition(
  position: CanonicalOverlayPosition,
  frame: PreviewFrameSize,
): PreviewPoint {
  return {
    xPx: position.x * frame.width,
    yPx: position.y * frame.height,
  };
}

/**
 * Canonical → preview CSS percentages for `left`/`top` with a
 * `translate(-50%, -50%)` center anchor. Equivalent to `toPreviewPosition`
 * expressed as a percentage of the frame; kept separate because overlay
 * styles position with `%`, not `px`.
 */
export function toPreviewPercent(
  position: CanonicalOverlayPosition,
): PreviewPercent {
  return {
    xPercent: position.x * 100,
    yPercent: position.y * 100,
  };
}

/**
 * Canonical → video pixels. Same meaning as `toPreviewPosition`, scaled to
 * the output frame (`targetWidth`/`targetHeight`). The renderer clips the
 * result at the frame boundary; it must not clamp the canonical input
 * (clamp removal belongs to Phase 4).
 */
export function toVideoPosition(
  position: CanonicalOverlayPosition,
  frame: VideoFrameSize,
): VideoPoint {
  return {
    xPx: position.x * frame.width,
    yPx: position.y * frame.height,
  };
}

/**
 * Preview drag delta (screen px) → canonical delta. Inverse of the position
 * scale for one axis: `canonical = start + deltaPx / framePx`.
 */
export function previewDeltaToCanonical(
  deltaPx: number,
  framePx: number,
): number {
  return deltaPx / framePx;
}
