// Phase 3 verification: freeform text must not acquire viewport-induced
// wrapping, while automatic subtitle layout stays untouched.
// Run: `node scripts/verify-phase3-text-layout.mjs`
//
// The repo has no React component test harness, so this script verifies the
// Phase 3 invariants structurally against `src/components/VideoCanvas.tsx`:
//   Test A — freeform style imposes no viewport text box and never wraps.
//   Test B — explicit newlines are preserved (`white-space: pre`).
//   Test C — the style path is pure (no canonical-state writes) and the
//            canvas frame remains the clipping boundary.
//   Test D — automatic/manual subtitle layout is unchanged.
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const canvasPath = join(root, "src", "components", "VideoCanvas.tsx");
const source = readFileSync(canvasPath, "utf8");

// Extract the span of a `const NAME = useCallback(/useMemo(` call via
// balanced scanning that skips strings, templates, and comments.
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

// --- Test A: no viewport-imposed text box, no automatic wrapping. ---
assert.ok(!freeform.includes("maxWidth"), "freeform style must not set maxWidth");
assert.ok(!freeform.includes("max-width"), "freeform style must not set max-width");
assert.ok(
  !freeform.includes("overflowWrap"),
  "freeform style must not set overflowWrap",
);
assert.ok(
  !freeform.includes("overflow-wrap"),
  "freeform style must not set overflow-wrap",
);
assert.ok(!freeform.includes("wordBreak"), "freeform style must not set wordBreak");
assert.ok(!freeform.includes("word-break"), "freeform style must not set word-break");
assert.ok(
  !/textOverflow\s*:/.test(freeform),
  "freeform style must not truncate with text-overflow",
);

// --- Test B: explicit newlines preserved, automatic wrapping off. ---
assert.ok(
  /whiteSpace:\s*"pre"/.test(freeform),
  'freeform whiteSpace must be exactly "pre" (explicit newlines kept, no auto-wrap)',
);

// --- Test C: style path is pure; canvas frame still clips. ---
for (const writer of [
  "applyTextOverlay(",
  "updateTextLayer(",
  "onTextOverlayChange(",
]) {
  assert.ok(
    !freeform.includes(writer),
    `freeform style path must not call ${writer} (no canonical mutation)`,
  );
}
assert.ok(
  source.includes('overflow: "hidden"'),
  "canvas frame must retain overflow:hidden as the clipping boundary",
);

// --- Test D: subtitle layout untouched. ---
// Manual subtitles keep their margin-derived width cap; automatic subtitles
// keep the bottom/left/right safe-area box with intentional wrapping.
assert.ok(
  subtitles.includes("maxWidth: `calc(100% - ${marginH * 2}px)`"),
  "manual subtitle maxWidth cap must remain",
);
assert.ok(subtitles.includes("bottom: marginV"), "auto subtitle bottom margin must remain");
assert.ok(subtitles.includes("left: marginH"), "auto subtitle left margin must remain");
assert.ok(subtitles.includes("right: marginH"), "auto subtitle right margin must remain");
assert.ok(!subtitles.includes("whiteSpace"), "subtitle whiteSpace behavior must not change");

console.log("Phase 3 text-layout verification passed.");
