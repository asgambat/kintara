import test from "node:test";
import assert from "node:assert/strict";

import { lanesForWidth } from "../src/lib/grid-lanes.ts";

test("lane count matches auto-fill arithmetic", () => {
  // 1200px grid, 180px cards, 16px gap: (1200+16)/(180+16) = 6.2 -> 6 lanes.
  assert.equal(lanesForWidth(1200, 180, 16), 6);
  // Narrow phone: a single column.
  assert.equal(lanesForWidth(360, 180, 16), 1);
  // Exact fit: (600+16)/(140+16) = 3.9 -> 3 lanes.
  assert.equal(lanesForWidth(600, 140, 16), 3);
});

test("garbage dimensions degrade to a single lane", () => {
  assert.equal(lanesForWidth(0, 180, 16), 1);
  assert.equal(lanesForWidth(-50, 180, 16), 1);
  assert.equal(lanesForWidth(Number.NaN, 180, 16), 1);
  assert.equal(lanesForWidth(1200, 0, 16), 1);
  assert.equal(lanesForWidth(1200, Number.NaN, 16), 1);
});
