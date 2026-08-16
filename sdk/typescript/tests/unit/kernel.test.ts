import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  kernelCanonicalJson,
  kernelIdempotencyKeyHex,
  kernelMergeField,
  kernelThreeWayDiff,
} from "../../src/kernel.js";
import { MergeContext } from "../../src/application/policy/index.js";
import type { JsonValue } from "../../src/domain/types.js";

type Vector = {
  canonicalId: string;
  operation: string;
  payload: JsonValue;
  canonicalPayloadJson: string;
  expectedHex: string;
};

const fixtureUrl = new URL(
  "../../../../spec/vectors/idempotency-vectors.json",
  import.meta.url,
);
const vectors = JSON.parse(
  readFileSync(fileURLToPath(fixtureUrl), "utf8"),
) as Vector[];

describe("kernel wrapper", () => {
  it("diffs with provenance", () => {
    const log = kernelThreeWayDiff({ qty: 1 }, { qty: 2 }, { qty: 1 });
    expect(log.changes[0]).toEqual({
      path: "qty",
      oldValue: 1,
      newFromA: 2,
      newFromB: undefined,
      source: "a",
    });
  });

  it("carries a null-clear as a real value", () => {
    const ancestor = { status: "draft" };
    const a = { status: null };
    const b = { status: "draft" };

    const kernelLog = kernelThreeWayDiff(ancestor, a, b);

    expect(kernelLog.changes[0]).toEqual({
      path: "status",
      oldValue: "draft",
      newFromA: null,
      newFromB: undefined,
      source: "a",
    });
  });

  it("canonicalizes with sorted keys", () => {
    expect(kernelCanonicalJson({ b: 1, a: 2 })).toBe('{"a":2,"b":1}');
  });

  it("matches every golden idempotency vector", () => {
    for (const v of vectors) {
      expect(
        kernelIdempotencyKeyHex(v.canonicalId, v.operation, v.payload),
      ).toBe(v.expectedHex);
    }
  });

  it("merges additive via declaration", () => {
    const out = kernelMergeField(
      { path: "qty", oldValue: 1, newFromA: 3, newFromB: 4, source: "both" },
      { kind: "additive" },
      new MergeContext("x", "y"),
    );
    expect(out).toEqual({ kind: "Resolved", value: 6 });
  });

  it("round-trips a null-clear through kernelMergeField without throwing", () => {
    const { changes } = kernelThreeWayDiff(
      { status: "draft" },
      { status: null },
      { status: "draft" },
    );
    const out = kernelMergeField(
      changes[0]!,
      { kind: "owned_by", system: "x" },
      new MergeContext("x", "y"),
    );
    expect(out).toEqual({ kind: "Resolved", value: null });
  });

  it("surfaces an Error for an inconsistent change", () => {
    expect(() =>
      kernelMergeField(
        {
          path: "qty",
          oldValue: 1,
          newFromA: undefined,
          newFromB: undefined,
          source: "a",
        },
        { kind: "additive" },
        new MergeContext("x", "y"),
      ),
    ).toThrow();
  });
});
