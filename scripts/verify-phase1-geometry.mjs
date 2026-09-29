// Phase 1 verification: canonical overlay geometry (video-space normalization).
// Run: `node scripts/verify-phase1-geometry.mjs` (Node 22.6+ strips types).
// Mirrors the Rust unit tests in `src-tauri/src/video/overlay_geometry.rs`.
import { strict as assert } from "node:assert";
import {
  isFiniteOverlayCoordinate,
  isFiniteOverlayPosition,
  previewDeltaToCanonical,
  toPreviewPercent,
  toPreviewPosition,
  toVideoPosition,
} from "../src/utils/overlayGeometry.ts";

const approx = (a, b, eps = 1e-6) => Math.abs(a - b) <= eps;

// Center: x = 0.5, y = 0.5 at 1080x1920 video and 270x480 preview.
{
  const video = toVideoPosition({ x: 0.5, y: 0.5 }, { width: 1080, height: 1920 });
  assert.ok(approx(video.xPx, 540), `video center x: ${video.xPx}`);
  assert.ok(approx(video.yPx, 960), `video center y: ${video.yPx}`);
  const preview = toPreviewPosition({ x: 0.5, y: 0.5 }, { width: 270, height: 480 });
  assert.ok(approx(preview.xPx, 135), `preview center x: ${preview.xPx}`);
  assert.ok(approx(preview.yPx, 240), `preview center y: ${preview.yPx}`);
  const percent = toPreviewPercent({ x: 0.5, y: 0.5 });
  assert.equal(percent.xPercent, 50);
  assert.equal(percent.yPercent, 50);
}

// Off-canvas finite values survive unclamped through the canonical layer.
{
  assert.ok(approx(toVideoPosition({ x: -0.1, y: 0.5 }, { width: 1080, height: 1920 }).xPx, -108, 1e-3));
  assert.ok(approx(toVideoPosition({ x: 1.1, y: 0.5 }, { width: 1080, height: 1920 }).xPx, 1188, 1e-3));
  assert.ok(approx(toVideoPosition({ x: 0.5, y: 1.1 }, { width: 1080, height: 1920 }).yPx, 2112, 1e-3));
  assert.equal(toPreviewPercent({ x: -0.1, y: 1.1 }).xPercent, -10);
  assert.ok(approx(toPreviewPercent({ x: -0.1, y: 1.1 }).yPercent, 110));
}

// Normalized meaning is resolution-independent (preview/video share it).
for (const width of [720, 1080, 1280, 1920]) {
  const px = toVideoPosition({ x: 0.75, y: 0.5 }, { width, height: 1920 }).xPx;
  assert.ok(approx(px / width, 0.75), `width ${width}: ${px}`);
}
{
  // previewX / previewWidth === videoX / videoWidth === canonical x
  const canonical = { x: 0.75, y: 0.25 };
  const preview = toPreviewPosition(canonical, { width: 270, height: 480 });
  const video = toVideoPosition(canonical, { width: 1080, height: 1920 });
  assert.ok(approx(preview.xPx / 270, canonical.x));
  assert.ok(approx(video.xPx / 1080, canonical.x));
  assert.ok(approx(preview.yPx / 480, canonical.y));
  assert.ok(approx(video.yPx / 1920, canonical.y));
}

// Finite-value guard: NaN/Infinity rejected, outside-[0,1] accepted.
{
  assert.equal(isFiniteOverlayCoordinate(Number.NaN), false);
  assert.equal(isFiniteOverlayCoordinate(Number.POSITIVE_INFINITY), false);
  assert.equal(isFiniteOverlayCoordinate(-0.2), true);
  assert.equal(isFiniteOverlayCoordinate(1.2), true);
  assert.equal(isFiniteOverlayPosition({ x: 0.5, y: Number.NaN }), false);
  assert.equal(isFiniteOverlayPosition({ x: -0.1, y: 1.1 }), true);
}

// Drag delta inversion: 27px on a 270px frame shifts canonical x by 0.1.
assert.ok(approx(previewDeltaToCanonical(27, 270), 0.1));

console.log("Phase 1 geometry verification passed.");
