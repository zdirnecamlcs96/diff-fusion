import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { Additive } from "../../../../src/application/policy/additive.js";
import {
  MergeContext,
  type FieldChange,
} from "../../../../src/application/policy/index.js";

function ctx(): MergeContext {
  return new MergeContext("a", "b");
}

/**
 * Build a `FieldChange` matching what threeWayDiff would produce for a
 * single numeric field `q`. Keeps the test decoupled from live three-way
 * diff computation while exercising every branch of Additive directly.
 */
function change(
  anc: number | string,
  a: number | string | undefined,
  b: number | string | undefined,
): FieldChange {
  const aMoved = a !== undefined && a !== anc;
  const bMoved = b !== undefined && b !== anc;
  const source = aMoved && bMoved ? "both" : aMoved ? "a" : "b";
  return {
    path: "q",
    oldValue: anc,
    newFromA: aMoved ? a : undefined,
    newFromB: bMoved ? b : undefined,
    source,
  };
}

describe("Additive", () => {
  it("one side's move passes through", () => {
    const out = new Additive().merge(change(10, 13, 10), ctx());
    expect(out).toEqual({ kind: "Resolved", value: 13 });
  });

  it("both sides' deltas accumulate", () => {
    // anc=10, a=+3 -> 13, b=+2 -> 12, merged = 10 + 3 + 2 = 15
    const out = new Additive().merge(change(10, 13, 12), ctx());
    expect(out).toEqual({ kind: "Resolved", value: 15 });
  });

  it("both decrement accumulates negative", () => {
    // anc=10, a=-2 -> 8, b=-3 -> 7, merged = 10 - 2 - 3 = 5
    const out = new Additive().merge(change(10, 8, 7), ctx());
    expect(out).toEqual({ kind: "Resolved", value: 5 });
  });

  it("non-numeric is a conflict", () => {
    const out = new Additive().merge(change("ten", "eleven", "ten"), ctx());
    expect(out.kind).toBe("Conflict");
  });

  it("name() is 'additive'", () => {
    expect(new Additive().name()).toBe("additive");
  });

  describe("property tests", () => {
    const deltaArb = fc.integer({ min: -10_000, max: 10_000 });
    const ancArb = fc.integer({ min: -1_000_000, max: 1_000_000 });

    it("commutative: swapping A and B does not change the merged value", () => {
      fc.assert(
        fc.property(ancArb, deltaArb, deltaArb, (anc, da, db) => {
          fc.pre(da !== 0 || db !== 0); // change() needs at least one side to move
          const aVal = anc + da;
          const bVal = anc + db;
          const p = new Additive();
          const c1 = change(anc, aVal, bVal);
          const c2 = change(anc, bVal, aVal);
          const r1 = p.merge(c1, ctx());
          const r2 = p.merge(c2, ctx());
          return JSON.stringify(r1) === JSON.stringify(r2);
        }),
      );
    });

    it("associative: ((anc + da) + db) + dc == (anc + da + db + dc) reachable by pairwise merges", () => {
      fc.assert(
        fc.property(ancArb, deltaArb, deltaArb, deltaArb, (anc, da, db, dc) => {
          fc.pre(da !== 0 || db !== 0); // first change() needs a move
          fc.pre(da + db !== 0 || dc !== 0); // second change() needs a move
          const p = new Additive();
          // Merge (A, B) first -> intermediate value that encodes anc + da + db.
          const step1 = p.merge(change(anc, anc + da, anc + db), ctx());
          if (step1.kind !== "Resolved" || typeof step1.value !== "number") {
            return false;
          }
          // Treat the merged value as the new A side, delta_c as B.
          const step2 = p.merge(change(anc, step1.value, anc + dc), ctx());
          // Compare against the all-in-one sum.
          const expected = anc + da + db + dc;
          return (
            step2.kind === "Resolved" &&
            typeof step2.value === "number" &&
            Math.abs(step2.value - expected) < 1e-9
          );
        }),
      );
    });
  });
});
