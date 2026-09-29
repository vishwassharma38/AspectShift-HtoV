// TEST-ONLY phase suite for roadmap Tests 6.1-6.7.
//
// Walks each numbered test's exact canonical values through the real code
// path stage by stage:
//
//   canonical input
//     -> normalize (compiled real utils: textOverlay / subtitleOverlay)
//     -> JSON save/load round-trip (real persistence path shape)
//     -> preview conversion (real overlayGeometry helpers)
//     -> renderer conversion (real overlayGeometry helpers, same arithmetic
//        the Rust renderer uses; exact ASS serialization is asserted by the
//        Rust ass_writer tests cited below)
//
// Run: `node scripts/verify-tests-6-1-6-7.mjs`
// Requires the project's own TypeScript compiler (node_modules) to compile
// the utils under test; no new test framework is introduced.
import { strict as assert } from "node:assert";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = mkdtempSync(join(tmpdir(), "tests-6-1-6-7-"));
let normalizeTextOverlay;
let normalizeSubtitleOverlay;
let geo;
try {
  execFileSync(
    process.execPath,
    [
      join(root, "node_modules", "typescript", "bin", "tsc"),
      join("src", "utils", "textOverlay.ts"),
      join("src", "utils", "subtitleOverlay.ts"),
      join("src", "utils", "overlayGeometry.ts"),
      "--outDir",
      outDir,
      "--module",
      "commonjs",
      "--target",
      "es2022",
      "--moduleResolution",
      "node",
      "--skipLibCheck",
    ],
    { cwd: root, stdio: "pipe" },
  );
  // tsc mirrors the src/ tree: outputs land in <outDir>/utils/.
  ({ normalizeTextOverlay } = await import(
    pathToFileURL(join(outDir, "utils", "textOverlay.js")).href
  ));
  ({ normalizeSubtitleOverlay } = await import(
    pathToFileURL(join(outDir, "utils", "subtitleOverlay.js")).href
  ));
  geo = await import(
    pathToFileURL(join(outDir, "utils", "overlayGeometry.js")).href
  );
} catch (error) {
  rmSync(outDir, { recursive: true, force: true });
  throw error;
}

const approx = (a, b, eps = 1e-6) => Math.abs(a - b) <= eps;
const cleanup = () => rmSync(outDir, { recursive: true, force: true });

const textLayerChain = (x, y, text = "I love this", fontSize = 48) => {
  const canonical = { x, y };
  const normalized = normalizeTextOverlay({
    panelOpen: true,
    layers: [{ id: "l1", text, fontSize, x, y }],
    selectedLayerIds: ["l1"],
  }).layers[0];
  // Real JSON persistence round-trip, then normalize again (load path).
  const reloaded = normalizeTextOverlay(
    JSON.parse(JSON.stringify(normalizeTextOverlay({
      panelOpen: true,
      layers: [{ id: "l1", text, fontSize, x, y }],
      selectedLayerIds: ["l1"],
    }))),
  ).layers[0];
  return { canonical, normalized, reloaded };
};

try {
  // --- Test 6.1: centered overlay (x=0.5, y=0.5, fontSize=48). ---
  {
    const { normalized, reloaded } = textLayerChain(0.5, 0.5);
    assert.equal(normalized.x, 0.5);
    assert.equal(normalized.y, 0.5);
    assert.equal(normalized.fontSize, 48);
    assert.equal(reloaded.x, 0.5);
    // Preview resolves to 50%/50%.
    const percent = geo.toPreviewPercent({ x: 0.5, y: 0.5 });
    assert.equal(percent.xPercent, 50);
    assert.equal(percent.yPercent, 50);
    // Renderer resolves to frame center on 1920x1080.
    const video = geo.toVideoPosition(
      { x: 0.5, y: 0.5 },
      { width: 1920, height: 1080 },
    );
    assert.equal(video.xPx, 960);
    assert.equal(video.yPx, 540);
    // Preview font scaling: 48 x scale stays mathematical (Phase 5).
    assert.ok(approx(geo.toPreviewFontSize(48, 0.5), 24));
    console.log("6.1 Centered overlay: PASS (behavioral)");
  }

  // --- Test 6.2: negative X (x=-0.1, y=0.5). ---
  {
    const { normalized, reloaded } = textLayerChain(-0.1, 0.5);
    assert.equal(normalized.x, -0.1); // survives normalization
    assert.equal(reloaded.x, -0.1); // survives save/load
    assert.equal(reloaded.y, 0.5);
    // Preview conversion preserves the negative fraction.
    const preview = geo.toPreviewPosition(
      { x: -0.1, y: 0.5 },
      { width: 1080, height: 1920 },
    );
    assert.ok(approx(preview.xPx / 1080, -0.1));
    // Renderer conversion on 1920x1080: x=-192, y=540 (signed ASS
    // serialization of the same values is asserted by Rust test
    // `subtitle_ass_manual_position_preserves_signed_off_canvas` and
    // `text_overlay_ass_preserves_signed_off_canvas_positions`).
    const video = geo.toVideoPosition(
      { x: -0.1, y: 0.5 },
      { width: 1920, height: 1080 },
    );
    assert.ok(approx(video.xPx, -192, 1e-3));
    assert.equal(video.yPx, 540);
    assert.ok(video.xPx < 0, "negative coordinate must remain negative");
    console.log("6.2 Negative X: PASS (behavioral; clipping: structural, see 6.9)");
  }

  // --- Test 6.3: X > 1 (x=1.1, y=0.5). ---
  {
    const { normalized, reloaded } = textLayerChain(1.1, 0.5);
    assert.equal(normalized.x, 1.1);
    assert.equal(reloaded.x, 1.1);
    const preview = geo.toPreviewPercent({ x: 1.1, y: 0.5 });
    assert.ok(approx(preview.xPercent, 110));
    const video = geo.toVideoPosition(
      { x: 1.1, y: 0.5 },
      { width: 1920, height: 1080 },
    );
    assert.ok(approx(video.xPx, 2112, 1e-3));
    assert.ok(
      Number.isInteger(Math.round(video.xPx)) && video.xPx < 2 ** 31,
      "no unsigned wraparound; fits signed 32-bit",
    );
    console.log("6.3 X > 1: PASS (behavioral; clipping: structural, see 6.9)");
  }

  // --- Test 6.4: preview resizing (800px vs 1200px widths). ---
  {
    const canonical = Object.freeze({ x: 0.25, y: 0.75, fontSize: 48 });
    const before = { ...canonical };
    const repA = geo.toPreviewPosition(
      { x: canonical.x, y: canonical.y },
      { width: 800, height: 1422 },
    );
    const repB = geo.toPreviewPosition(
      { x: canonical.x, y: canonical.y },
      { width: 1200, height: 2133 },
    );
    // Screen representation changes proportionally...
    assert.ok(approx(repB.xPx / repA.xPx, 1.5));
    // ...but means the same normalized location...
    assert.ok(approx(repA.xPx / 800, canonical.x));
    assert.ok(approx(repB.xPx / 1200, canonical.x));
    // ...and canonical state is untouched.
    assert.deepEqual({ ...canonical }, before);
    // Font representation scales; canonical fontSize does not.
    assert.ok(approx(geo.toPreviewFontSize(canonical.fontSize, 0.5), 24));
    assert.equal(canonical.fontSize, 48);
    // (Render-triggered non-mutation is additionally locked by the
    // Phase 2 structural script: no overlay-state writers in effects.)
    console.log("6.4 Preview resizing: PASS (behavioral math + frozen state)");
  }

  // --- Test 6.5: long freeform text + explicit newline. ---
  {
    const long = "I love this gigantic sentence that keeps going past the frame";
    const normalizedLong = normalizeTextOverlay({
      layers: [{ id: "l1", text: long, x: 0.5, y: 0.5 }],
    }).layers[0];
    assert.equal(
      normalizedLong.text,
      long,
      "long single line must survive normalization verbatim (no wrapping there)",
    );
    const multi = "I love this\ngigantic sentence";
    const normalizedMulti = normalizeTextOverlay({
      layers: [{ id: "l1", text: multi, x: 0.5, y: 0.5 }],
    }).layers[0];
    assert.equal(
      normalizedMulti.text,
      multi,
      "explicit newline must survive normalization",
    );
    const roundTripped = normalizeTextOverlay(
      JSON.parse(
        JSON.stringify(
          normalizeTextOverlay({ layers: [{ id: "l1", text: multi }] }),
        ),
      ),
    ).layers[0];
    assert.equal(roundTripped.text, multi, "newline survives save/load");
    // (No auto-wrap + frame-clip rendering is locked structurally by the
    // Phase 3 script: white-space:pre, no maxWidth/overflowWrap, canvas
    // overflow:hidden. Browser-measured wrapping is untested here: §9.)
    console.log("6.5 Long freeform text: PASS (behavioral data path)");
  }

  // --- Test 6.6: off-canvas persistence (x=-0.15, y=1.10). ---
  {
    const saved = JSON.stringify(
      normalizeTextOverlay({
        panelOpen: true,
        layers: [{ id: "l1", text: "Hi", x: -0.15, y: 1.1 }],
        selectedLayerIds: ["l1"],
      }),
    );
    const reloaded = normalizeTextOverlay(JSON.parse(saved)).layers[0];
    assert.equal(reloaded.x, -0.15);
    assert.equal(reloaded.y, 1.1);
    const sub = normalizeSubtitleOverlay({ manualPosition: true, x: -0.15, y: 1.1 });
    const reloadedSub = normalizeSubtitleOverlay(
      JSON.parse(JSON.stringify(sub)),
    );
    assert.equal(reloadedSub.x, -0.15);
    assert.equal(reloadedSub.y, 1.1);
    console.log("6.6 Off-canvas persistence: PASS (behavioral, real JSON path)");
  }

  // --- Test 6.7: preview/output geometry parity (x=0.75 @1920 wide). ---
  {
    const out = geo.toVideoPosition({ x: 0.75, y: 0.5 }, { width: 1920, height: 1080 });
    assert.equal(out.xPx / 1920, 0.75);
    const preview = geo.toPreviewPosition(
      { x: 0.75, y: 0.5 },
      { width: 480, height: 270 },
    );
    assert.ok(approx(preview.xPx / 480, 0.75));
    console.log("6.7 Preview/output parity: PASS (behavioral)");
  }

  console.log("Tests 6.1-6.7 chain suite: ALL PASS");
} finally {
  cleanup();
}
