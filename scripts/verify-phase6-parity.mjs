// Phase 6 verification: preview and renderer represent the SAME logical
// overlay geometry through different engines (browser CSS vs ASS/libass).
// Run: `node scripts/verify-phase6-parity.mjs` (Node 22.6+ strips types).
//
// Covers the prompt section 13 matrix at the logical-equivalence level:
//   Position - canonical fractions agree on preview and renderer frames,
//     including edges (0/1) and off-canvas (<0/>1) on both axes.
//   Scale - Phase 5 invariant re-asserted through the same lens.
//   Anchor - preview center anchor and ASS \an5 co-exist (structural).
//   Clipping - geometry is never repositioned at the boundary (structural).
//   Resize - two preview sizes give proportional px, identical fractions,
//     and untouched canonical state.
//   Text layout - explicit-newline contract both sides (structural).
// Plus guards: no fontScale migration, no renderer correction factors.
//
// Like the Phase 1-5 scripts, behavioral parts execute the real helpers;
// structural parts assert source invariants - neither proves
// browser-rendered pixels or FFmpeg pixel output.
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  toPreviewFontSize,
  toPreviewPosition,
  toVideoPosition,
} from "../src/utils/overlayGeometry.ts";

const approx = (a, b, eps = 1e-6) => Math.abs(a - b) <= eps;

// --- Position: same logical location on preview AND renderer frames. ---
// Frames deliberately differ in aspect (portrait preview, portrait video,
// landscape video) to prove aspect-independent equivalence.
{
  const frames = {
    previewPortrait: { width: 270, height: 480 },
    videoPortrait: { width: 1080, height: 1920 },
    videoLandscape: { width: 1920, height: 1080 },
  };
  const coords = [0, 0.5, 1, -0.1, 1.1];
  for (const x of coords) {
    for (const y of coords) {
      const preview = toPreviewPosition({ x, y }, frames.previewPortrait);
      const videoP = toVideoPosition({ x, y }, frames.videoPortrait);
      const videoL = toVideoPosition({ x, y }, frames.videoLandscape);
      assert.ok(
        approx(preview.xPx / frames.previewPortrait.width, x),
        `preview x fraction for (${x},${y})`,
      );
      assert.ok(
        approx(preview.yPx / frames.previewPortrait.height, y),
        `preview y fraction for (${x},${y})`,
      );
      assert.ok(
        approx(videoP.xPx / frames.videoPortrait.width, x),
        `portrait-renderer x fraction for (${x},${y})`,
      );
      assert.ok(
        approx(videoP.yPx / frames.videoPortrait.height, y),
        `portrait-renderer y fraction for (${x},${y})`,
      );
      assert.ok(
        approx(videoL.xPx / frames.videoLandscape.width, x),
        `landscape-renderer x fraction for (${x},${y})`,
      );
      assert.ok(
        approx(videoL.yPx / frames.videoLandscape.height, y),
        `landscape-renderer y fraction for (${x},${y})`,
      );
    }
  }
  // Spot-check absolute renderer values behind the fractions.
  const neg = toVideoPosition({ x: -0.1, y: 0.5 }, frames.videoPortrait);
  assert.ok(approx(neg.xPx, -108, 1e-3));
  const beyond = toVideoPosition({ x: 1.1, y: 0.5 }, frames.videoPortrait);
  assert.ok(approx(beyond.xPx, 1188, 1e-3));
}

// --- Scale: Phase 5 invariant through the parity lens. ---
assert.ok(approx(toPreviewFontSize(48, 0.5), 24));
assert.ok(approx(toPreviewFontSize(4, 0.5), 2));

// --- Resize: representation changes, canonical state does not. ---
{
  const canonical = Object.freeze({ x: -0.1, y: 0.5, fontSize: 48 });
  const small = toPreviewPosition(
    { x: canonical.x, y: canonical.y },
    { width: 270, height: 480 },
  );
  const large = toPreviewPosition(
    { x: canonical.x, y: canonical.y },
    { width: 405, height: 720 },
  );
  assert.ok(approx(large.xPx / small.xPx, 1.5), "px scales with viewport");
  assert.ok(
    approx(large.xPx / 405, small.xPx / 270),
    "normalized meaning identical across sizes",
  );
  assert.ok(approx(toPreviewFontSize(canonical.fontSize, 0.5), 24));
  assert.ok(approx(toPreviewFontSize(canonical.fontSize, 0.75), 36));
  assert.equal(canonical.x, -0.1);
  assert.equal(canonical.y, 0.5);
  assert.equal(canonical.fontSize, 48);
}

// --- Structural: anchors, clipping, newline contract, no corrections. ---
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const canvas = readFileSync(
  join(root, "src", "components", "VideoCanvas.tsx"),
  "utf8",
);
const assWriter = readFileSync(
  join(root, "src-tauri", "src", "subtitles", "ass_writer.rs"),
  "utf8",
);
const filterBuilder = readFileSync(
  join(root, "src-tauri", "src", "video", "filter_builder.rs"),
  "utf8",
);

// Anchor parity: preview center anchor and ASS \an5 describe the same center.
assert.ok(
  canvas.includes("translate(-50%, -50%)"),
  "preview center anchor must exist",
);
assert.ok(
  assWriter.includes("\\an5"),
  "renderer center anchor (ASS \\an5) must exist",
);
assert.ok(
  filterBuilder.includes("-overlay_w/2"),
  "logo center anchor must exist",
);

// Clipping parity: frame clips, geometry is not repositioned.
assert.ok(canvas.includes('overflow: "hidden"'), "preview frame must clip");
assert.ok(
  !assWriter.includes("x.clamp") && !assWriter.includes("y.clamp"),
  "renderer must not clamp coordinates",
);
assert.ok(
  !filterBuilder.includes(".clamp(0.0"),
  "logo renderer must not clamp coordinates",
);

// Text layout: explicit newlines are the only breaks on both sides.
assert.ok(
  /whiteSpace:\s*"pre"/.test(canvas),
  "preview must preserve explicit newlines without auto-wrap",
);
assert.ok(
  assWriter.includes("replace"),
  "ASS writer must escape explicit newlines to \\N",
);

// No fontScale migration anywhere in the frontend.
for (const file of [
  "src/components/VideoCanvas.tsx",
  "src/utils/overlayGeometry.ts",
  "src/utils/textOverlay.ts",
  "src/utils/subtitleOverlay.ts",
  "src/App.tsx",
  "src/types/backend.ts",
]) {
  const content = readFileSync(join(root, file), "utf8");
  assert.ok(!content.includes("fontScale"), `${file} must not introduce fontScale`);
}

// No renderer correction factors: the only `fontSize * <decimal>` products
// in the preview are the documented per-style letter-spacing values.
{
  const products = [...canvas.matchAll(/fontSize\s*\*\s*[\d.]+/g)].map(
    (m) => m[0],
  );
  assert.ok(products.length > 0, "expected letter-spacing products to exist");
  for (const product of products) {
    assert.ok(
      product === "fontSize * 0.04" || product === "fontSize * 0.03",
      `unexpected font-size factor (possible correction hack): ${product}`,
    );
  }
  assert.ok(
    !/fontSize\s*[+-]\s*\d/.test(canvas),
    "no additive font-size correction may exist",
  );
}

console.log("Phase 6 parity verification passed.");
