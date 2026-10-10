// Phase 5 verification: preview font size is a pure mathematical scale of
// the canonical video-space fontSize, with no arbitrary minimum floor.
// Run: `node scripts/verify-phase5-font-scaling.mjs` (Node 22.6+ strips types).
//
// Covers the prompt's Tests A–F behaviorally against the real
// `toPreviewFontSize` helper, plus structural checks that both preview style
// paths use it and no `Math.max(8/12, …)` floor remains on rendered text.
// Like the Phase 1–3 scripts, the structural part asserts source invariants,
// not browser-rendered pixels.
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { toPreviewFontSize } from "../src/utils/overlayGeometry.ts";

const approx = (a, b, eps = 1e-9) => Math.abs(a - b) <= eps;

// --- Test A: normal font. 48 × 0.5 = 24. ---
assert.ok(approx(toPreviewFontSize(48, 0.5), 24));

// --- Test B: tiny font. 4 × 0.5 = 2 (must NOT become 8 or 12). ---
assert.ok(approx(toPreviewFontSize(4, 0.5), 2));

// --- Test C: scale 1.0 is identity. 48 × 1.0 = 48. ---
assert.equal(toPreviewFontSize(48, 1.0), 48);

// --- Test D: same canonical size at two preview scales. ---
assert.ok(approx(toPreviewFontSize(48, 0.5), 24));
assert.ok(approx(toPreviewFontSize(48, 0.75), 36));

// --- Test E: canonical state is never mutated by the transform. ---
{
  const canonical = Object.freeze({ fontSize: 48, x: 0.5, y: 0.5 });
  assert.ok(approx(toPreviewFontSize(canonical.fontSize, 0.5), 24));
  assert.ok(approx(toPreviewFontSize(canonical.fontSize, 0.75), 36));
  assert.equal(canonical.fontSize, 48);
  assert.equal(canonical.x, 0.5);
  assert.equal(canonical.y, 0.5);
}

// --- Test F: no arbitrary floor overrides sub-floor results. ---
assert.ok(toPreviewFontSize(4, 0.5) < 8, "2 must stay below the old floor of 8");
assert.ok(toPreviewFontSize(12, 0.25) < 8, "3 must stay below the old floor of 8");
assert.ok(toPreviewFontSize(20, 0.5) < 12, "10 must stay below the old floor of 12");

// --- Structural: both preview style paths use the helper, floors are gone. ---
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const canvasPath = join(root, "src", "components", "VideoCanvas.tsx");
const source = readFileSync(canvasPath, "utf8");

function hookSpan(text, name, hook) {
  const start = text.indexOf(`const ${name} = ${hook}(`);
  assert.ok(start !== -1, `expected to find ${name}`);
  let i = start + `const ${name} = ${hook}(`.length;
  let depth = 1;
  let state = "code";
  while (i < text.length && depth > 0) {
    const ch = text[i];
    const next = text[i + 1];
    if (state === "code") {
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
      else if (ch === "'") state = "sq";
      else if (ch === '"') state = "dq";
      else if (ch === "`") state = "tpl";
      else if (ch === "/" && next === "/") state = "line";
      else if (ch === "/" && next === "*") state = "block";
    } else if (state === "sq" || state === "dq" || state === "tpl") {
      const quote = state === "sq" ? "'" : state === "dq" ? '"' : "`";
      if (ch === "\\") i++;
      else if (ch === quote) state = "code";
    } else if (state === "line") {
      if (ch === "\n") state = "code";
    } else if (state === "block") {
      if (ch === "*" && next === "/") {
        state = "code";
        i++;
      }
    }
    i++;
  }
  assert.ok(depth === 0, `unbalanced ${name} call`);
  return text.slice(start, i);
}

const freeform = hookSpan(source, "getTextLayerStyle", "useCallback");
const subtitles = hookSpan(source, "subtitleStyle", "useMemo");

assert.ok(
  freeform.includes("toPreviewFontSize("),
  "freeform style must derive font size via toPreviewFontSize",
);
assert.ok(
  !freeform.includes("Math.max(8"),
  "freeform style must not floor the rendered font size",
);
assert.ok(
  subtitles.includes("toPreviewFontSize("),
  "subtitle style must derive font size via toPreviewFontSize",
);
assert.ok(
  !subtitles.includes("Math.max(12"),
  "subtitle style must not floor the rendered font size",
);
assert.ok(
  !source.includes("Math.max(8,") && !source.includes("Math.max(12,"),
  "no rendered-text font floor may remain in VideoCanvas.tsx",
);

console.log("Phase 5 font-scaling verification passed.");
