import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_HIGHLIGHT_COLOR,
  safeHighlightColor,
} from "../src/lib/highlight-color.ts";

test("plain hex and rgb values pass through", () => {
  assert.equal(safeHighlightColor("#410186"), "#410186");
  assert.equal(
    safeHighlightColor("rgba(139, 92, 246, 0.35)"),
    "rgba(139, 92, 246, 0.35)",
  );
  assert.equal(safeHighlightColor("rgb(0, 0, 0)"), "rgb(0, 0, 0)");
});

test("missing or empty values fall back", () => {
  assert.equal(safeHighlightColor(null), DEFAULT_HIGHLIGHT_COLOR);
  assert.equal(safeHighlightColor(undefined), DEFAULT_HIGHLIGHT_COLOR);
  assert.equal(safeHighlightColor("   "), DEFAULT_HIGHLIGHT_COLOR);
});

test("markup and CSS breakouts fall back", () => {
  for (const evil of [
    '"; background: url(evil);',
    '"onmouseover="alert(1)',
    "</mark><script>alert(1)</script>",
    "expression(alert(1))",
    "red",
    "rgba(1,2,3);color:red",
  ]) {
    assert.equal(
      safeHighlightColor(evil),
      DEFAULT_HIGHLIGHT_COLOR,
      evil,
    );
  }
});
