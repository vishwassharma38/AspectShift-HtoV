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
 * video-space coordinates and the frame clips visibility. Drag interaction
 * is unbounded (no min/max drag bounds): screen-px deltas convert via
 * `previewDeltaToCanonical` with no clamping. Remaining boundaries
 * (unrelated numeric validation such as opacity / font size / scale,
 * automatic-subtitle margin layout) are interaction or product policy,
 * not geometry.
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

/**
 * Phase 5: canonical video-space font size → preview pixels.
 *
 * Pure scale with no minimum floor: `previewFontSize = canonicalFontSize ×
 * previewScale`. The persisted `fontSize` keeps its existing meaning and
 * units (video-space px); only this derived representation changes with the
 * preview size. Canonical state is never mutated — pass numbers in, get a
 * number out.
 */
export function toPreviewFontSize(
  canonicalFontSize: number,
  previewScale: number,
): number {
  return canonicalFontSize * previewScale;
}

/**
 * Bounding-box resize handle identifiers, shared by every overlay type that
 * offers direct bounding-box manipulation (images, text).
 */
export type OverlayResizeHandle =
  | "n"
  | "s"
  | "e"
  | "w"
  | "ne"
  | "nw"
  | "se"
  | "sw";

/**
 * Resize-handle drag delta (screen px) → box-size delta (screen px).
 *
 * Pure function of (handle, dx, dy): corner handles split the combined drag
 * evenly across both axes, edge handles use their own axis. Callers convert
 * the result into their own canonical size representation (image `scale`,
 * text `fontSize`), so aspect handling stays domain-specific while the
 * handle geometry is defined exactly once.
 */
export function resizeHandleDeltaPx(
  handle: OverlayResizeHandle,
  dx: number,
  dy: number,
): number {
  switch (handle) {
    case "e":
      return dx;
    case "w":
      return -dx;
    case "s":
      return dy;
    case "n":
      return -dy;
    case "se":
      return (dx + dy) / 2;
    case "nw":
      return (-dx - dy) / 2;
    case "ne":
      return (dx - dy) / 2;
    case "sw":
      return (-dx + dy) / 2;
  }
}

/**
 * Rotation-handle drag → rotation delta (degrees).
 *
 * Pure function of (startAngleDeg, currentAngleDeg): the shortest signed
 * sweep between the two pointer angles, normalized to [-180, 180] so
 * crossing the ±180° seam stays smooth. Callers add it to their own stored
 * rotation; clamping to the domain's rotation bounds stays domain-specific.
 */
export function rotationDeltaDeg(
  startAngleDeg: number,
  currentAngleDeg: number,
): number {
  let delta = currentAngleDeg - startAngleDeg;
  if (delta > 180) delta -= 360;
  if (delta < -180) delta += 360;
  return delta;
}

/** Pointer position → angle (degrees) around a center point. */
export function pointerAngleDeg(
  clientX: number,
  clientY: number,
  centerX: number,
  centerY: number,
): number {
  return (Math.atan2(clientY - centerY, clientX - centerX) * 180) / Math.PI;
}
