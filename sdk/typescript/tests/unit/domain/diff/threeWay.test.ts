import fc from "fast-check";
import { describe, expect, it } from "vitest";
import {
  changelogIsEmpty,
  threeWayDiff,
} from "../../../../src/domain/diff/threeWay.js";

describe("threeWayDiff", () => {
  it("identical inputs produce an empty changelog", () => {
    const v = { price: 10, qty: 5 };
    const log = threeWayDiff(v, v, v);
    expect(changelogIsEmpty(log)).toBe(true);
  });

  it("only A changed → source is 'a'", () => {
    const anc = { price: 10 };
    const a = { price: 12 };
    const b = { price: 10 };
    const log = threeWayDiff(anc, a, b);
    expect(log.changes.length).toBe(1);
    const c = log.changes[0]!;
    expect(c.path).toBe("price");
    expect(c.source).toBe("a");
    expect(c.newFromA).toBe(12);
    expect(c.newFromB).toBeUndefined();
    expect(c.oldValue).toBe(10);
  });

  it("only B changed → source is 'b'", () => {
    const anc = { price: 10 };
    const a = { price: 10 };
    const b = { price: 15 };
    const log = threeWayDiff(anc, a, b);
    expect(log.changes.length).toBe(1);
    const c = log.changes[0]!;
    expect(c.source).toBe("b");
    expect(c.newFromA).toBeUndefined();
    expect(c.newFromB).toBe(15);
  });

  it("both changed → source is 'both' even when values agree", () => {
    const anc = { price: 10 };
    const a = { price: 12 };
    const b = { price: 12 };
    const log = threeWayDiff(anc, a, b);
    expect(log.changes.length).toBe(1);
    expect(log.changes[0]!.source).toBe("both");
  });

  it("both changed to different values", () => {
    const anc = { price: 10 };
    const a = { price: 12 };
    const b = { price: 15 };
    const log = threeWayDiff(anc, a, b);
    expect(log.changes.length).toBe(1);
    const c = log.changes[0]!;
    expect(c.source).toBe("both");
    expect(c.newFromA).toBe(12);
    expect(c.newFromB).toBe(15);
  });

  it("independent fields from each side", () => {
    const anc = { price: 10, qty: 5, name: "widget" };
    const a = { price: 12, qty: 5, name: "widget" };
    const b = { price: 10, qty: 7, name: "widget" };
    const log = threeWayDiff(anc, a, b);
    expect(log.changes.length).toBe(2);
    const byPath = new Map(log.changes.map((c) => [c.path, c]));
    expect(byPath.get("price")!.source).toBe("a");
    expect(byPath.get("qty")!.source).toBe("b");
  });

  it("changes sorted by path", () => {
    const anc = { a: 1, b: 2, c: 3 };
    const a = { a: 10, b: 20, c: 30 };
    const b = { a: 1, b: 2, c: 3 };
    const log = threeWayDiff(anc, a, b);
    expect(log.changes.map((c) => c.path)).toEqual(["a", "b", "c"]);
  });

  it("nested paths are dotted", () => {
    const anc = { pricing: { amount: 10 } };
    const a = { pricing: { amount: 12 } };
    const b = { pricing: { amount: 10 } };
    const log = threeWayDiff(anc, a, b);
    expect(log.changes[0]!.path).toBe("pricing.amount");
  });

  describe("property tests", () => {
    it("threeWayDiff(x, x, x) is always empty", () => {
      fc.assert(
        fc.property(fc.integer(), (seed) => {
          const v = { n: seed };
          const log = threeWayDiff(v, v, v);
          return changelogIsEmpty(log);
        }),
      );
    });

    it("A-only moves produce only source='a'", () => {
      fc.assert(
        fc.property(
          fc.integer(),
          fc.integer(),
          (ancVal, aVal) => {
            fc.pre(ancVal !== aVal);
            const anc = { n: ancVal };
            const a = { n: aVal };
            const b = { n: ancVal };
            const log = threeWayDiff(anc, a, b);
            return log.changes.every((c) => c.source === "a");
          },
        ),
      );
    });

    it("B-only moves produce only source='b'", () => {
      fc.assert(
        fc.property(
          fc.integer(),
          fc.integer(),
          (ancVal, bVal) => {
            fc.pre(ancVal !== bVal);
            const anc = { n: ancVal };
            const a = { n: ancVal };
            const b = { n: bVal };
            const log = threeWayDiff(anc, a, b);
            return log.changes.every((c) => c.source === "b");
          },
        ),
      );
    });

    it("swapping A and B inverts source (both unchanged)", () => {
      fc.assert(
        fc.property(
          fc.integer(),
          fc.integer(),
          fc.integer(),
          (ancVal, aVal, bVal) => {
            const anc = { n: ancVal };
            const a = { n: aVal };
            const b = { n: bVal };
            const forward = threeWayDiff(anc, a, b);
            const swapped = threeWayDiff(anc, b, a);
            if (forward.changes.length !== swapped.changes.length) return false;
            for (let i = 0; i < forward.changes.length; i++) {
              const f = forward.changes[i]!;
              const s = swapped.changes[i]!;
              if (f.path !== s.path) return false;
              const expected =
                f.source === "a" ? "b" : f.source === "b" ? "a" : "both";
              if (s.source !== expected) return false;
            }
            return true;
          },
        ),
      );
    });
  });
});
