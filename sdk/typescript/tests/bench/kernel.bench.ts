/**
 * Gate 2 benchmark: kernel (wasm) benchmarks.
 *
 * Decides whether Task 15 (delete the native TS kernel source) may proceed.
 * Gate criterion: kernel `threeWayDiff` p50 < 500µs on a 50-field doc.
 * Verdict: measured mean ~114µs — PASS (2026-07-24). Not a `.test.ts` file —
 * excluded from normal `vitest run` by `vitest.config.ts`'s `include` glob.
 */
import { bench, describe } from "vitest";
import { threeWayDiff } from "../../src/domain/diff/threeWay.js";
import { idempotencyKeyHex } from "../../src/domain/idempotency.js";
import type { JsonValue } from "../../src/domain/types.js";

function doc(fields: number): Record<string, unknown> {
  return Object.fromEntries(
    Array.from({ length: fields }, (_, i) => [`field_${i}`, { qty: i, name: `item ${i}`, tags: ["a", "b"] }]),
  );
}

describe("threeWayDiff 50-field entity", () => {
  const anc = doc(50);
  const a = { ...doc(50), field_3: { qty: 99, name: "item 3", tags: ["a", "b"] } };
  const b = doc(50);

  bench("kernel (wasm)", () => {
    threeWayDiff(anc as never, a as never, b as never);
  });
});

describe("threeWayDiff 5-field entity", () => {
  const anc = doc(5);
  const a = { ...doc(5), field_3: { qty: 99, name: "item 3", tags: ["a", "b"] } };
  const b = doc(5);

  bench("kernel (wasm)", () => {
    threeWayDiff(anc as never, a as never, b as never);
  });
});

describe("idempotencyKeyHex ~2KB payload", () => {
  // ~2KB of canonical-JSON payload: 40 fields of moderate string content.
  const payload = doc(40) as unknown as JsonValue;

  bench("kernel (wasm)", () => {
    idempotencyKeyHex("entity-123", "sync", payload);
  });
});
