// Phase 2 verification: preview rendering must not mutate canonical overlay geometry.
// Run: `node scripts/verify-phase2-no-mutation.mjs`
//
// The repo has no React component test harness, so this script verifies the
// Phase 2 invariant structurally against `src/components/VideoCanvas.tsx`:
//   1. `clampTextToFrame` (the automatic frame-containment rewriter) is gone.
//   2. No `useLayoutEffect`/`useEffect` body invokes a canonical overlay-state
//      writer — i.e. no render/resize/layout pass can rewrite x/y.
//   3. User-driven update paths (drag, typing, selection, editing) are intact.
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const canvasPath = join(root, "src", "components", "VideoCanvas.tsx");
const source = readFileSync(canvasPath, "utf8");

// --- 1. The state-mutating frame-containment helper is gone. ---
assert.ok(
  !source.includes("clampTextToFrame"),
  "clampTextToFrame must not exist (definition, call, or comment reference)",
);

// --- 2. No effect body writes canonical overlay state. ---
// Overlay-state writers: each ultimately calls onTextOverlayChange /
// onSubtitleOverlayChange / onLogoChange and rewrites persisted x/y or layers.
const OVERLAY_STATE_WRITERS = [
  "applyTextOverlay(",
  "onTextOverlayChange(",
  "updateTextLayer(",
  "updateSubtitleOverlay(",
  "onSubtitleOverlayChange(",
  "onLogoChange(",
];

// Extract spans of every useEffect/useLayoutEffect call via balanced scanning
// that skips over string literals, template literals, and comments.
function effectSpans(text) {
  const spans = [];
  const hookPattern = /use(LayoutEffect|Effect)\s*\(/g;
  let match;
  while ((match = hookPattern.exec(text)) !== null) {
    let i = match.index + match[0].length; // just after the opening paren
    let depth = 1;
    let state = "code"; // code | sq | dq | tpl | line | block
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
      } else if (state === "sq") {
        if (ch === "\\") i++;
        else if (ch === "'") state = "code";
      } else if (state === "dq") {
        if (ch === "\\") i++;
        else if (ch === '"') state = "code";
      } else if (state === "tpl") {
        if (ch === "\\") i++;
        else if (ch === "`") state = "code";
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
    assert.ok(depth === 0, `unbalanced effect call at index ${match.index}`);
    spans.push(text.slice(match.index, i));
  }
  return spans;
}

const spans = effectSpans(source);
assert.ok(spans.length > 0, "expected to find effect calls in VideoCanvas.tsx");
for (const span of spans) {
  for (const writer of OVERLAY_STATE_WRITERS) {
    assert.ok(
      !span.includes(writer),
      `render-triggered effect must not call ${writer}:\n${span.slice(0, 160)}...`,
    );
  }
}

// --- 3. Legitimate user-driven update paths are preserved. ---
// Drag movement writes x/y because the user moved the overlay.
assert.ok(
  /handleTextPointerMove[\s\S]*?updateTextLayer\(\s*drag\.layerId,\s*\{\s*x,\s*y\s*\}/.test(source),
  "text drag must still update x/y (user-driven movement)",
);
assert.ok(source.includes("handleLogoPointerMove"), "logo drag handler must remain");
assert.ok(
  source.includes("handleSubtitlePointerMove"),
  "subtitle drag handler must remain",
);
// Typing/editing still updates text content.
assert.ok(source.includes("commitTextEditing"), "text editing commit must remain");
// Selection still works.
assert.ok(source.includes("selectTextLayer"), "layer selection must remain");
// Phase 1 canonical helpers remain in use on the preview path.
assert.ok(source.includes("toPreviewPercent("), "Phase 1 preview transform must remain");
assert.ok(
  source.includes("previewDeltaToCanonical("),
  "Phase 1 drag-delta transform must remain",
);

console.log(
  `Phase 2 no-mutation verification passed (${spans.length} effect bodies checked, no overlay-state writers).`,
);
