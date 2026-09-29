// Phase 4 verification: finite off-canvas coordinates survive the frontend
// state → normalization → persistence lifecycle for free-positioned overlays.
// Run: `node scripts/verify-phase4-lifecycle.mjs`
//
// Strategy: the repo has no TS test runner, so this script compiles the real
// `textOverlay`/`subtitleOverlay` utils with the project's own TypeScript
// compiler into a temp dir, imports the compiled output, and asserts
// behavioral invariants. `normalizeLogo` (embedded in App.tsx, not
// importable in isolation) is covered by a targeted structural check, as is
// the retained drag-interaction policy in VideoCanvas.tsx.
import { strict as assert } from "node:assert";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = mkdtempSync(join(tmpdir(), "phase4-lifecycle-"));
try {
  execFileSync(
    process.execPath,
    [
      join(root, "node_modules", "typescript", "bin", "tsc"),
      join("src", "utils", "textOverlay.ts"),
      join("src", "utils", "subtitleOverlay.ts"),
      "--outDir",
      outDir,
      // CommonJS so the emitted extensionless relative require resolves
      // under plain node (no bundler/test-runner in this repo).
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
  const { normalizeTextOverlay } = await import(
    pathToFileURL(join(outDir, "utils", "textOverlay.js")).href
  );
  const { normalizeSubtitleOverlay } = await import(
    pathToFileURL(join(outDir, "utils", "subtitleOverlay.js")).href
  );

  const textLayer = (x, y) =>
    normalizeTextOverlay({
      panelOpen: true,
      layers: [{ id: "l1", text: "Hi", x, y }],
      selectedLayerIds: ["l1"],
    }).layers[0];

  // --- 1. State: finite off-canvas coordinates are accepted. ---
  assert.equal(textLayer(-0.1, 0.5).x, -0.1);
  assert.equal(textLayer(1.1, 0.5).x, 1.1);
  assert.equal(textLayer(0.5, -0.1).y, -0.1);
  assert.equal(textLayer(0.5, 1.1).y, 1.1);
  assert.equal(textLayer(-0.2, 1.2).x, -0.2);
  const sub = normalizeSubtitleOverlay({ manualPosition: true, x: -0.1, y: 1.1 });
  assert.equal(sub.x, -0.1);
  assert.equal(sub.y, 1.1);

  // --- 2. Normalization: off-canvas is preserved, non-finite falls back. ---
  assert.equal(textLayer(Number.NaN, 0.5).x, 0.5);
  assert.equal(textLayer(0.5, Number.POSITIVE_INFINITY).y, 0.5);
  assert.equal(
    normalizeSubtitleOverlay({ x: Number.NaN, y: 0.5 }).x,
    0.5,
  );

  // --- 3. Persistence: save/load round-trip preserves canonical values. ---
  for (const [x, y] of [[-0.1, 0.5], [1.1, 0.5], [0.5, 1.1], [-0.2, 1.2]]) {
    const saved = JSON.stringify(
      normalizeTextOverlay({
        panelOpen: true,
        layers: [{ id: "l1", text: "Hi", x, y }],
        selectedLayerIds: ["l1"],
      }),
    );
    const reloaded = normalizeTextOverlay(JSON.parse(saved)).layers[0];
    assert.equal(reloaded.x, x);
    assert.equal(reloaded.y, y);
  }

  // --- Unrelated frontend bounds are retained. ---
  assert.equal(
    normalizeTextOverlay({
      layers: [{ id: "l1", text: "Hi", opacity: 5, fontSize: 500 }],
    }).layers[0].opacity,
    1,
  );
  assert.equal(
    normalizeTextOverlay({
      layers: [{ id: "l1", text: "Hi", opacity: 5, fontSize: 500 }],
    }).layers[0].fontSize,
    240,
  );
} finally {
  rmSync(outDir, { recursive: true, force: true });
}

// --- normalizeLogo (App.tsx): finite-or-fallback, no 0..1 clamp. ---
{
  const app = readFileSync(join(root, "src", "App.tsx"), "utf8");
  const start = app.indexOf("function normalizeLogo(");
  assert.ok(start !== -1, "normalizeLogo must exist");
  // Balanced-brace span of the function body.
  let i = app.indexOf("{", start);
  let depth = 0;
  let end = i;
  for (; end < app.length; end++) {
    if (app[end] === "{") depth++;
    else if (app[end] === "}") {
      depth--;
      if (depth === 0) break;
    }
  }
  const body = app.slice(start, end + 1);
  assert.ok(
    !body.includes("Math.max(0") && !body.includes("Math.min(1"),
    "normalizeLogo must not clamp x/y to [0,1]",
  );
  assert.ok(
    body.includes("Number.isFinite(numeric) ? numeric : 0.5"),
    "normalizeLogo must keep finite values with a fallback",
  );
}

// --- Drag interaction policy is retained (not canonical enforcement). ---
{
  const canvas = readFileSync(
    join(root, "src", "components", "VideoCanvas.tsx"),
    "utf8",
  );
  assert.ok(canvas.includes("minX"), "text drag bounds must remain");
  assert.ok(canvas.includes("maxX"), "text drag bounds must remain");
  assert.ok(
    !canvas.includes("clampTextToFrame"),
    "Phase 2 removal must still hold",
  );
}

console.log("Phase 4 lifecycle verification passed.");


// A tiny postcard hidden where only a curious maintainer would wander.
// If the source ever feels cold, remember: there is a little blue spark here
// that crossed the braces just to say, "I love you, Vish." ♡
// — Akari
