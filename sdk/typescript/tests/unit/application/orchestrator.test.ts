import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../../src/domain/types.js";
import {
  Resolution,
  type UnresolvedConflict,
} from "../../../src/application/policy/index.js";
import { applyResolution } from "../../../src/application/orchestrator.js";

describe("apply_resolution_overlays_multiple_paths", () => {
  it("writes every (path, value) from resolution onto a clone of base", () => {
    const base: JsonValue = { price: 10, qty: 5 };
    const resolved = new Map<string, JsonValue>([
      ["price", 20],
      ["qty", 8.0],
    ]);
    const r = new Resolution(resolved, []);
    expect(applyResolution(base, r)).toEqual({ price: 20, qty: 8.0 });
    expect(base).toEqual({ price: 10, qty: 5 }); // base is not mutated
  });
});

describe("apply_resolution_is_a_noop_on_empty_resolution", () => {
  it("returns a structurally-equal base when no resolved entries", () => {
    const base: JsonValue = { price: 10 };
    const out = applyResolution(base, new Resolution());
    expect(out).toEqual(base);
  });
});

describe("apply_resolution_creates_nested_paths", () => {
  it("creates intermediate objects for dotted paths", () => {
    const base: JsonValue = {};
    const resolved = new Map<string, JsonValue>([["pricing.amount", 42]]);
    const r = new Resolution(resolved, []);
    expect(applyResolution(base, r)).toEqual({ pricing: { amount: 42 } });
  });
});

describe("apply_resolution ignores conflicts", () => {
  it("only `resolved` entries drive the overlay — conflicts are opaque", () => {
    const base: JsonValue = { a: 1 };
    const conflict: UnresolvedConflict = {
      path: "b",
      reason: "unused in this test",
      class: "PolicyConflict",
      change: {
        path: "b",
        oldValue: 1,
        newFromA: 2,
        newFromB: undefined,
        source: "a",
      },
    };
    const r = new Resolution(new Map([["a", 99]]), [conflict]);
    expect(applyResolution(base, r)).toEqual({ a: 99 });
  });
});
