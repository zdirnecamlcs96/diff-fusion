import { describe, expect, it } from "vitest";
import { compareJson } from "../../../src/domain/compare.js";

describe("compareJson", () => {
  it("returns no diffs for identical objects", () => {
    const a = { product_name: "Widget", product_price: 19.99 };
    const b = { product_name: "Widget", product_price: 19.99 };
    expect(compareJson(a, b)).toEqual([]);
  });

  it("reports changed leaves", () => {
    const a = { product_name: "Widget", product_price: 19.99, stock: 100 };
    const b = { product_name: "Gadget", product_price: 19.99, stock: 95 };
    const diffs = compareJson(a, b);
    expect(diffs.length).toBe(2);
    const paths = diffs.map(([p]) => p);
    expect(paths).toContain("product_name");
    expect(paths).toContain("stock");
  });

  it("treats 5 and 5.0 as equal (numeric equality)", () => {
    const diffs = compareJson({ price: 5 }, { price: 5.0 });
    expect(diffs).toEqual([]);
  });

  it("walks nested objects and emits dotted paths", () => {
    const a = { product: { name: "Widget", price: 19.99 } };
    const b = { product: { name: "Widget", price: 24.99 } };
    const diffs = compareJson(a, b);
    expect(diffs.length).toBe(1);
    expect(diffs[0]![0]).toBe("product.price");
    expect(diffs[0]![1]).toEqual([19.99, 24.99]);
  });

  it("treats missing keys as null on the missing side", () => {
    const a = { keep: 1, gone: "x" };
    const b = { keep: 1, added: "y" };
    const diffs = compareJson(a, b);
    const map = new Map(diffs.map(([p, v]) => [p, v]));
    expect(map.get("gone")).toEqual(["x", null]);
    expect(map.get("added")).toEqual([null, "y"]);
  });

  it("reports scalar vs object mismatch as a single leaf change", () => {
    const a = { outer: 1 };
    const b = { outer: { nested: 2 } };
    const diffs = compareJson(a, b);
    expect(diffs.length).toBe(1);
    expect(diffs[0]![0]).toBe("outer");
    expect(diffs[0]![1]).toEqual([1, { nested: 2 }]);
  });

  it("reports array mismatch as a single leaf change at the array path", () => {
    const a = { items: [1, 2, 3] };
    const b = { items: [1, 2, 4] };
    const diffs = compareJson(a, b);
    expect(diffs.length).toBe(1);
    expect(diffs[0]![0]).toBe("items");
  });

  it("emits deterministic (sorted) path order", () => {
    const a = { z: 1, a: 1, m: 1 };
    const b = { z: 2, a: 2, m: 2 };
    const diffs = compareJson(a, b);
    const paths = diffs.map(([p]) => p);
    expect(paths).toEqual(["a", "m", "z"]);
  });

  it("handles top-level scalar comparison", () => {
    expect(compareJson(1, 1)).toEqual([]);
    expect(compareJson("x", "y")).toEqual([["", ["x", "y"]]]);
  });

  it("considers booleans unequal to numbers (no coercion)", () => {
    const diffs = compareJson({ v: true }, { v: 1 });
    expect(diffs.length).toBe(1);
  });
});
