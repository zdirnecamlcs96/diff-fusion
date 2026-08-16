import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { threeWayDiff } from "../../../../src/domain/diff/threeWay.js";
import type { JsonValue } from "../../../../src/domain/types.js";
import {
  MergeContext,
  type MergeOutcome,
} from "../../../../src/application/policy/index.js";
import { OwnedBy } from "../../../../src/application/policy/ownedBy.js";

function ctx(): MergeContext {
  return new MergeContext("sys_a", "sys_b");
}

describe("OwnedBy.name", () => {
  it("returns 'owned_by' (matches Rust)", () => {
    expect(new OwnedBy("sys_a").name()).toBe("owned_by");
  });
});

describe("owner_a_wins_when_a_moves", () => {
  it("resolves to A's new value when A owns and moved", () => {
    const anc: JsonValue = { x: 1 };
    const a: JsonValue = { x: 5 };
    const b: JsonValue = { x: 1 };
    const log = threeWayDiff(anc, a, b);
    const out = new OwnedBy("sys_a").merge(log.changes[0]!, ctx());
    expect(out).toEqual<MergeOutcome>({ kind: "Resolved", value: 5 });
  });
});

describe("non_owner_change_reverts_to_ancestor", () => {
  it("reverts B's attempted change when A owns", () => {
    const anc: JsonValue = { x: 1 };
    const a: JsonValue = { x: 1 };
    const b: JsonValue = { x: 99 };
    const log = threeWayDiff(anc, a, b);
    const out = new OwnedBy("sys_a").merge(log.changes[0]!, ctx());
    expect(out).toEqual<MergeOutcome>({ kind: "Resolved", value: 1 });
  });
});

describe("owner_wins_when_both_move", () => {
  it("resolves to owner B's value when both moved", () => {
    const anc: JsonValue = { x: 1 };
    const a: JsonValue = { x: 5 };
    const b: JsonValue = { x: 99 };
    const log = threeWayDiff(anc, a, b);
    const out = new OwnedBy("sys_b").merge(log.changes[0]!, ctx());
    expect(out).toEqual<MergeOutcome>({ kind: "Resolved", value: 99 });
  });
});

describe("unknown_owner_is_a_conflict", () => {
  it("returns Conflict when owner label matches neither side", () => {
    const anc: JsonValue = { x: 1 };
    const a: JsonValue = { x: 2 };
    const b: JsonValue = { x: 1 };
    const log = threeWayDiff(anc, a, b);
    const out = new OwnedBy("unknown_system").merge(log.changes[0]!, ctx());
    expect(out.kind).toBe("Conflict");
    if (out.kind === "Conflict") {
      expect(out.reason).toContain("does not match");
      expect(out.reason).toContain("unknown_system");
      expect(out.reason).toContain("sys_a");
      expect(out.reason).toContain("sys_b");
    }
  });
});

describe("OwnedBy property: non-owner-only edits revert to ancestor", () => {
  it("for any scalar, if only the non-owner moved, result equals ancestor", () => {
    // Draw ancestor + non-owner-new-value; owner is "sys_a", non-owner is "sys_b".
    // Since A does NOT move (a equals ancestor), FieldChange.newFromA is absent,
    // so OwnedBy should revert to ancestor.
    fc.assert(
      fc.property(
        fc.oneof(fc.integer(), fc.string(), fc.boolean()),
        fc.oneof(fc.integer(), fc.string(), fc.boolean()),
        (ancestor, bNew) => {
          fc.pre(!Object.is(ancestor, bNew)); // require actual change
          const anc: JsonValue = { x: ancestor as JsonValue };
          const a: JsonValue = { x: ancestor as JsonValue };
          const b: JsonValue = { x: bNew as JsonValue };
          const log = threeWayDiff(anc, a, b);
          // B moved, A did not → exactly one change.
          expect(log.changes).toHaveLength(1);
          const out = new OwnedBy("sys_a").merge(log.changes[0]!, ctx());
          expect(out).toEqual<MergeOutcome>({
            kind: "Resolved",
            value: ancestor as JsonValue,
          });
        },
      ),
      { numRuns: 200 },
    );
  });

  it("symmetrically: owner=B, only A moved → revert to ancestor", () => {
    fc.assert(
      fc.property(
        fc.oneof(fc.integer(), fc.string()),
        fc.oneof(fc.integer(), fc.string()),
        (ancestor, aNew) => {
          fc.pre(!Object.is(ancestor, aNew));
          const anc: JsonValue = { x: ancestor as JsonValue };
          const a: JsonValue = { x: aNew as JsonValue };
          const b: JsonValue = { x: ancestor as JsonValue };
          const log = threeWayDiff(anc, a, b);
          expect(log.changes).toHaveLength(1);
          const out = new OwnedBy("sys_b").merge(log.changes[0]!, ctx());
          expect(out).toEqual<MergeOutcome>({
            kind: "Resolved",
            value: ancestor as JsonValue,
          });
        },
      ),
      { numRuns: 200 },
    );
  });

  it("owner's edit always wins regardless of non-owner's state", () => {
    fc.assert(
      fc.property(
        fc.integer(),
        fc.integer(),
        fc.oneof(fc.integer(), fc.constant(undefined)),
        (ancestor, ownerNew, nonOwnerMaybe) => {
          fc.pre(ancestor !== ownerNew);
          const anc: JsonValue = { x: ancestor };
          const a: JsonValue = { x: ownerNew };
          const b: JsonValue = {
            x: nonOwnerMaybe === undefined ? ancestor : nonOwnerMaybe,
          };
          const log = threeWayDiff(anc, a, b);
          // A always moved; B may or may not have moved.
          expect(log.changes).toHaveLength(1);
          const out = new OwnedBy("sys_a").merge(log.changes[0]!, ctx());
          expect(out).toEqual<MergeOutcome>({
            kind: "Resolved",
            value: ownerNew,
          });
        },
      ),
      { numRuns: 200 },
    );
  });
});
