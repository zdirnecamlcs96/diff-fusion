import { describe, expect, it } from "vitest";
import {
  InvariantSet,
  type Invariant,
  type InvariantOutcome,
} from "../../../../src/application/policy/invariants.js";
import type { JsonValue } from "../../../../src/domain/types.js";

const NoReceiveAgainstClosed: Invariant = {
  name: () => "no_receive_against_closed",
  check: (previous, candidate) => {
    const status = getString(candidate, "status") ?? "";
    if (status !== "closed") return { kind: "Pass" };
    const prevQty = getNumber(previous, "qty_recv") ?? 0;
    const candQty = getNumber(candidate, "qty_recv") ?? 0;
    if (candQty > prevQty) {
      return {
        kind: "Reject",
        reason: "cannot increase qty_recv on a closed entity",
      };
    }
    return { kind: "Pass" };
  },
};

const CapQty: Invariant = {
  name: () => "cap_qty",
  check: (_prev, candidate) => {
    const q = getNumber(candidate, "qty") ?? 0;
    if (q > 100) {
      const fixed = { ...(candidate as { [k: string]: JsonValue }), qty: 100 };
      return { kind: "Transform", value: fixed };
    }
    return { kind: "Pass" };
  },
};

function alwaysReject(name: string): Invariant {
  return {
    name: () => name,
    check: (): InvariantOutcome => ({
      kind: "Reject",
      reason: "by design",
    }),
  };
}

function getString(v: JsonValue, key: string): string | undefined {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return undefined;
  const x = (v as { [k: string]: JsonValue })[key];
  return typeof x === "string" ? x : undefined;
}

function getNumber(v: JsonValue, key: string): number | undefined {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return undefined;
  const x = (v as { [k: string]: JsonValue })[key];
  return typeof x === "number" ? x : undefined;
}

describe("InvariantSet", () => {
  it("passes when the invariant holds", () => {
    const set = new InvariantSet().with(NoReceiveAgainstClosed);
    const prev = { status: "open", qty_recv: 5 };
    const cand = { status: "open", qty_recv: 7 };
    expect(set.apply(prev, cand)).toEqual({ kind: "Pass" });
  });

  it("rejects when closed and qty increases, prefixing invariant name", () => {
    const set = new InvariantSet().with(NoReceiveAgainstClosed);
    const prev = { status: "closed", qty_recv: 5 };
    const cand = { status: "closed", qty_recv: 7 };
    const out = set.apply(prev, cand);
    expect(out.kind).toBe("Reject");
    if (out.kind === "Reject") {
      expect(out.reason).toContain("no_receive_against_closed");
      expect(out.reason).toContain("cannot increase");
    }
  });

  it("transform is visible to caller", () => {
    const set = new InvariantSet().with(CapQty);
    const prev = { qty: 50 };
    const cand = { qty: 150 };
    const out = set.apply(prev, cand);
    expect(out.kind).toBe("Transform");
    if (out.kind === "Transform") {
      expect(out.value).toEqual({ qty: 100 });
    }
  });

  it("first rejection stops evaluation", () => {
    const set = new InvariantSet()
      .with(alwaysReject("first"))
      .with(alwaysReject("second"));
    const out = set.apply({}, {});
    expect(out.kind).toBe("Reject");
    if (out.kind === "Reject") {
      expect(out.reason.startsWith("first")).toBe(true);
    }
  });

  it("empty set passes through", () => {
    const out = new InvariantSet().apply({ a: 1 }, { a: 1 });
    expect(out).toEqual({ kind: "Pass" });
  });

  it("chain: transform then pass leaves final Transform outcome", () => {
    const set = new InvariantSet().with(CapQty).with(NoReceiveAgainstClosed);
    // cand has qty=150 (triggers CapQty->100) and status=open (NoReceive passes).
    const out = set.apply({ qty: 0, status: "open" }, { qty: 150, status: "open" });
    expect(out.kind).toBe("Transform");
    if (out.kind === "Transform") {
      expect(out.value).toEqual({ qty: 100, status: "open" });
    }
  });

  it("chain: transform then reject surfaces rejection with rejecter's name", () => {
    // Transform first (CapQty trims qty to 100), then NoReceiveAgainstClosed
    // inspects the transformed candidate (status=closed, qty_recv bumped).
    const set = new InvariantSet().with(CapQty).with(NoReceiveAgainstClosed);
    const prev = { qty: 0, status: "closed", qty_recv: 5 };
    const cand = { qty: 150, status: "closed", qty_recv: 7 };
    const out = set.apply(prev, cand);
    expect(out.kind).toBe("Reject");
    if (out.kind === "Reject") {
      expect(out.reason.startsWith("no_receive_against_closed")).toBe(true);
    }
  });
});
