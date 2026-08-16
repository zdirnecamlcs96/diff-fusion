import { describe, expect, it } from "vitest";
import { Additive } from "../../../../src/application/policy/additive.js";
import { Append } from "../../../../src/application/policy/append.js";
import { LastWriteWins } from "../../../../src/application/policy/escapeHatch.js";
import { MergeContext } from "../../../../src/application/policy/index.js";
import { OwnedBy } from "../../../../src/application/policy/ownedBy.js";
import {
  StateMachine,
  StateTransition,
} from "../../../../src/application/policy/stateMachine.js";
import { SetByKey } from "../../../../src/application/policy/structural.js";
import { threeWayDiff } from "../../../../src/domain/diff/threeWay.js";
import type { JsonValue } from "../../../../src/domain/types.js";

function ctx(): MergeContext {
  return new MergeContext("x", "y");
}

describe("built-in policy toRef + kernel merge", () => {
  describe("Additive", () => {
    it("toRef", () => {
      expect(new Additive().toRef()).toEqual({ kind: "additive" });
    });

    it("merge routes through kernel and preserves semantics", () => {
      const out = new Additive().merge(
        { path: "qty", source: "both", oldValue: 1, newFromA: 3, newFromB: 4 },
        ctx(),
      );
      expect(out).toEqual({ kind: "Resolved", value: 6 });
    });
  });

  describe("OwnedBy", () => {
    it("toRef carries system", () => {
      expect(new OwnedBy("x").toRef()).toEqual({ kind: "owned_by", system: "x" });
    });

    it("merge routes through kernel and preserves semantics", () => {
      const anc: JsonValue = { price: 1 };
      const a: JsonValue = { price: 5 };
      const b: JsonValue = { price: 99 };
      const log = threeWayDiff(anc, a, b);
      const out = new OwnedBy("x").merge(log.changes[0]!, ctx());
      expect(out).toEqual({ kind: "Resolved", value: 5 });
    });
  });

  describe("Append", () => {
    it("toRef", () => {
      expect(new Append().toRef()).toEqual({ kind: "append" });
    });

    it("merge routes through kernel and preserves semantics", () => {
      const out = new Append().merge(
        {
          path: "notes",
          source: "both",
          oldValue: ["x"],
          newFromA: ["x", "y"],
          newFromB: ["x", "z"],
        },
        ctx(),
      );
      expect(out).toEqual({ kind: "Resolved", value: ["x", "y", "z"] });
    });
  });

  describe("StateMachine", () => {
    it("toRef carries transitions", () => {
      const sm = new StateMachine([new StateTransition("draft", "open")]);
      expect(sm.toRef()).toEqual({
        kind: "state_machine",
        transitions: [{ from: "draft", to: "open" }],
      });
    });

    it("merge routes through kernel and preserves semantics", () => {
      const sm = new StateMachine([new StateTransition("draft", "open")]);
      const anc: JsonValue = { status: "draft" };
      const a: JsonValue = { status: "open" };
      const log = threeWayDiff(anc, a, anc);
      const out = sm.merge(log.changes[0]!, ctx());
      expect(out).toEqual({ kind: "Resolved", value: "open" });
    });
  });

  describe("LastWriteWins", () => {
    it("toRef carries reason and timestamps", () => {
      const lww = new LastWriteWins("legacy support", 100, 200);
      expect(lww.toRef()).toEqual({
        kind: "last_write_wins",
        reason: "legacy support",
        timestamp_a: 100,
        timestamp_b: 200,
      });
    });

    it("merge routes through kernel and preserves semantics", () => {
      const anc: JsonValue = { x: 1 };
      const a: JsonValue = { x: 2 };
      const b: JsonValue = { x: 3 };
      const log = threeWayDiff(anc, a, b);
      const out = new LastWriteWins("no natural owner", 100, 200).merge(
        log.changes[0]!,
        ctx(),
      );
      expect(out).toEqual({ kind: "Resolved", value: 3 });
    });
  });

  describe("SetByKey", () => {
    it("toRef carries the full declaration shape", () => {
      const policy = new SetByKey(["sku"], "externalId", "internalId");
      expect(policy.toRef()).toEqual({
        kind: "set_by_key",
        identity: ["sku"],
        a_anchor: "externalId",
        b_anchor: "internalId",
        on_added_in_a: "Include",
        on_added_in_b: "Include",
        on_removed_in_a: "EscalateIfChanged",
        on_removed_in_b: "EscalateIfChanged",
        on_both_changed: "Escalate",
        prefer_a_on_field_conflict: true,
        nested: {},
      });
    });
  });
});
