import { describe, expect, it } from "vitest";
import { setAtPath } from "../../../src/domain/jsonPath.js";
import type { JsonValue } from "../../../src/domain/types.js";

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
