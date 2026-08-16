import { describe, expect, it } from "vitest";
import { Append } from "../../../../src/application/policy/append.js";
import {
  MergeContext,
  type FieldChange,
} from "../../../../src/application/policy/index.js";
import type { JsonValue } from "../../../../src/domain/types.js";

function ctx(): MergeContext {
  return new MergeContext("a", "b");
}

function shallowEqualArray(x: JsonValue, y: JsonValue): boolean {
  return JSON.stringify(x) === JSON.stringify(y);
}

function change(
  anc: JsonValue,
  a: JsonValue | undefined,
  b: JsonValue | undefined,
): FieldChange {
  const aMoved = a !== undefined && !shallowEqualArray(a, anc);
  const bMoved = b !== undefined && !shallowEqualArray(b, anc);
  const source = aMoved && bMoved ? "both" : aMoved ? "a" : "b";
  return {
    path: "notes",
    oldValue: anc,
    newFromA: aMoved ? a : undefined,
    newFromB: bMoved ? b : undefined,
    source,
  };
}

describe("Append", () => {
  it("A-only move returns A's array", () => {
    const out = new Append().merge(change(["x"], ["x", "y"], ["x"]), ctx());
    expect(out).toEqual({ kind: "Resolved", value: ["x", "y"] });
  });

  it("both additions concatenate (A's before B's)", () => {
    const out = new Append().merge(
      change(["x"], ["x", "y"], ["x", "z"]),
      ctx(),
    );
    expect(out).toEqual({ kind: "Resolved", value: ["x", "y", "z"] });
  });

  it("independent duplicates are kept", () => {
    // anc=[], a=["y"], b=["y"] → ["y","y"] — both sides independently added.
    const out = new Append().merge(change([], ["y"], ["y"]), ctx());
    expect(out).toEqual({ kind: "Resolved", value: ["y", "y"] });
  });

  it("non-array is a conflict", () => {
    const out = new Append().merge(
      change("text", "text plus", "text"),
      ctx(),
    );
    expect(out.kind).toBe("Conflict");
  });

  it("name() is 'append'", () => {
    expect(new Append().name()).toBe("append");
  });

  it("B-only move returns B's array", () => {
    const out = new Append().merge(change(["x"], ["x"], ["x", "z"]), ctx());
    expect(out).toEqual({ kind: "Resolved", value: ["x", "z"] });
  });

  it("preserves ancestor elements when both sides add new items", () => {
    const out = new Append().merge(
      change(["a", "b"], ["a", "b", "c"], ["a", "b", "d"]),
      ctx(),
    );
    expect(out).toEqual({ kind: "Resolved", value: ["a", "b", "c", "d"] });
  });

  it("deep-equality used for containment (object elements)", () => {
    // anc has {id:1}. A adds {id:2}, B adds {id:1} (already in anc) + {id:3}.
    // Only new items relative to anc should be appended.
    const out = new Append().merge(
      change(
        [{ id: 1 }],
        [{ id: 1 }, { id: 2 }],
        [{ id: 1 }, { id: 3 }],
      ),
      ctx(),
    );
    expect(out).toEqual({
      kind: "Resolved",
      value: [{ id: 1 }, { id: 2 }, { id: 3 }],
    });
  });
});
