import { describe, expect, it } from "vitest";
import { escapeSegment, setAtPath, splitPath } from "../../../src/domain/jsonPath.js";
import type { JsonValue } from "../../../src/domain/types.js";

describe("escapeSegment / splitPath", () => {
  it("plain segments round-trip like split-on-dot", () => {
    const joined = `${escapeSegment("a")}.${escapeSegment("b")}`;
    expect(joined).toBe("a.b");
    expect(splitPath(joined)).toEqual(["a", "b"]);
  });

  it("dotted key round-trips through escape and split, landing in the real key", () => {
    const key = "key_2025-01-01T00:00:00.000Z_demo";
    const joined = `items.${escapeSegment(key)}.name`;
    expect(joined).toBe("items.key_2025-01-01T00:00:00\\.000Z_demo.name");
    expect(splitPath(joined)).toEqual(["items", key, "name"]);

    const v: JsonValue = { items: { [key]: { name: "old" } } };
    setAtPath(v, joined, "new");
    expect(v).toEqual({ items: { [key]: { name: "new" } } });
    // No phantom branch from the unescaped prefix before the literal dot.
    expect((v as { items: Record<string, unknown> }).items["key_2025-01-01T00:00:00"]).toBeUndefined();
  });

  it("backslash key round-trips", () => {
    const key = "weird\\key";
    const joined = escapeSegment(key);
    expect(joined).toBe("weird\\\\key");
    expect(splitPath(joined)).toEqual([key]);
  });

  it("trailing lone backslash is kept literal deterministically", () => {
    expect(splitPath("a\\")).toEqual(["a\\"]);
  });
});

describe("setAtPath", () => {
  it("empty path is a no-op", () => {
    const v: JsonValue = { a: 1 };
    setAtPath(v, "", 99);
    expect(v).toEqual({ a: 1 });
  });

  it("leaf set overwrites existing value", () => {
    const v: JsonValue = { a: 1, b: 2 };
    setAtPath(v, "a", 99);
    expect(v).toEqual({ a: 99, b: 2 });
  });

  it("leaf set inserts missing key", () => {
    const v: JsonValue = { a: 1 };
    setAtPath(v, "b", 2);
    expect(v).toEqual({ a: 1, b: 2 });
  });

  it("nested path creates intermediate objects", () => {
    const v: JsonValue = {};
    setAtPath(v, "outer.inner.leaf", "x");
    expect(v).toEqual({ outer: { inner: { leaf: "x" } } });
  });

  it("nested path overwrites existing leaf", () => {
    const v: JsonValue = { outer: { inner: { leaf: "old" } } };
    setAtPath(v, "outer.inner.leaf", "new");
    expect(v).toEqual({ outer: { inner: { leaf: "new" } } });
  });

  it("non-object along path is replaced with an object so descent can continue", () => {
    const v: JsonValue = { outer: 42 };
    setAtPath(v, "outer.inner", "x");
    expect(v).toEqual({ outer: { inner: "x" } });
  });

  it("preserves sibling keys", () => {
    const v: JsonValue = {
      keep: "me",
      outer: { keep: "also", inner: 1 },
    };
    setAtPath(v, "outer.inner", 99);
    expect(v).toEqual({
      keep: "me",
      outer: { keep: "also", inner: 99 },
    });
  });

  it("stores non-object value types", () => {
    const v: JsonValue = {};
    setAtPath(v, "arr", [1, 2, 3]);
    setAtPath(v, "s", "hello");
    setAtPath(v, "n", 42);
    setAtPath(v, "b", true);
    setAtPath(v, "null", null);
    expect(v).toEqual({
      arr: [1, 2, 3],
      s: "hello",
      n: 42,
      b: true,
      null: null,
    });
  });
});
