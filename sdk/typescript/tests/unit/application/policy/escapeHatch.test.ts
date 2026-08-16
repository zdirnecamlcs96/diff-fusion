import { describe, expect, it } from "vitest";
import { LastWriteWins } from "../../../../src/application/policy/escapeHatch.js";
import { MergeContext } from "../../../../src/application/policy/index.js";
import { threeWayDiff } from "../../../../src/domain/diff/threeWay.js";

function ctx(): MergeContext {
  return new MergeContext("a", "b");
}

// ---------------------------------------------------------------------------
// Ported Rust #[test] blocks (src/application/policy/escape_hatch.rs)
// ---------------------------------------------------------------------------

describe("LastWriteWins — ported Rust tests", () => {
  it("reason_is_visible", () => {
    const lww = new LastWriteWins("legacy support", 10, 20);
    expect(lww.reason).toBe("legacy support");
  });

  it("both_moved_newer_timestamp_wins", () => {
    const anc = { x: 1 };
    const a = { x: 2 };
    const b = { x: 3 };
    const log = threeWayDiff(anc, a, b);

    const lww = new LastWriteWins("no natural owner", 100, 200);
    const out = lww.merge(log.changes[0]!, ctx());
    expect(out).toEqual({ kind: "Resolved", value: 3 });
  });

  it("tied_timestamps_escalate", () => {
    const anc = { x: 1 };
    const a = { x: 2 };
    const b = { x: 3 };
    const log = threeWayDiff(anc, a, b);

    const lww = new LastWriteWins("reason", 100, 100);
    const out = lww.merge(log.changes[0]!, ctx());
    expect(out.kind).toBe("Conflict");
  });

  it("single_side_move_is_trivial", () => {
    const anc = { x: 1 };
    const a = { x: 7 };
    const b = { x: 1 };
    const log = threeWayDiff(anc, a, b);

    const lww = new LastWriteWins("r", 0, 0);
    expect(lww.merge(log.changes[0]!, ctx())).toEqual({
      kind: "Resolved",
      value: 7,
    });
  });
});

// ---------------------------------------------------------------------------
// TS-side checks: name(), symmetric tie-break, B newer than A
// ---------------------------------------------------------------------------

describe("LastWriteWins — additional behaviour", () => {
  it("name() is 'last_write_wins'", () => {
    expect(new LastWriteWins("r", 0, 0).name()).toBe("last_write_wins");
  });

  it("B wins when timestampB > timestampA", () => {
    const log = threeWayDiff({ x: 1 }, { x: 2 }, { x: 3 });
    const out = new LastWriteWins("r", 100, 200).merge(log.changes[0]!, ctx());
    expect(out).toEqual({ kind: "Resolved", value: 3 });
  });

  it("A wins when timestampA > timestampB", () => {
    const log = threeWayDiff({ x: 1 }, { x: 2 }, { x: 3 });
    const out = new LastWriteWins("r", 200, 100).merge(log.changes[0]!, ctx());
    expect(out).toEqual({ kind: "Resolved", value: 2 });
  });

  it("tie reason string surfaces the policy name", () => {
    const log = threeWayDiff({ x: 1 }, { x: 2 }, { x: 3 });
    const out = new LastWriteWins("r", 5, 5).merge(log.changes[0]!, ctx());
    expect(out.kind).toBe("Conflict");
    if (out.kind === "Conflict") {
      expect(out.reason).toBe("last_write_wins: timestamps tie");
    }
  });

  it("constructor requires reason (type-level)", () => {
    // This is really a compile-time check: `new LastWriteWins()` and
    // `new LastWriteWins(undefined, 0, 0)` both fail `tsc`. We assert runtime
    // behaviour by confirming reason is stored as given and is publicly
    // readable on the instance.
    const lww = new LastWriteWins("compliance override", 1, 2);
    expect(lww.reason).toBe("compliance override");
    expect(lww.timestampA).toBe(1);
    expect(lww.timestampB).toBe(2);
  });
});
