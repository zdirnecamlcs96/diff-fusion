import fc from "fast-check";
import { describe, expect, it } from "vitest";
import {
  StateMachine,
  StateTransition,
} from "../../../../src/application/policy/stateMachine.js";
import {
  MergeContext,
  type FieldChange,
} from "../../../../src/application/policy/index.js";
import { threeWayDiff } from "../../../../src/domain/diff/threeWay.js";

function ctx(): MergeContext {
  return new MergeContext("a", "b");
}

function poStates(): StateMachine {
  return new StateMachine([
    new StateTransition("draft", "open"),
    new StateTransition("open", "closed"),
    new StateTransition("open", "cancelled"),
  ]);
}

// ---------------------------------------------------------------------------
// Ported Rust #[test] blocks (src/application/policy/state_machine.rs)
// ---------------------------------------------------------------------------

describe("StateMachine — ported Rust tests", () => {
  it("legal_transition_is_accepted", () => {
    const anc = { status: "draft" };
    const a = { status: "open" };
    const b = { status: "draft" };
    const log = threeWayDiff(anc, a, b);

    const out = poStates().merge(log.changes[0]!, ctx());
    expect(out).toEqual({ kind: "Resolved", value: "open" });
  });

  it("illegal_transition_is_rejected", () => {
    // closed -> draft is not allowed.
    const anc = { status: "closed" };
    const a = { status: "draft" };
    const b = { status: "closed" };
    const log = threeWayDiff(anc, a, b);

    const out = poStates().merge(log.changes[0]!, ctx());
    expect(out.kind).toBe("Conflict");
    if (out.kind === "Conflict") {
      expect(out.reason).toContain("illegal transition");
    }
  });

  it("both_agree_on_legal_transition", () => {
    const anc = { status: "open" };
    const a = { status: "closed" };
    const b = { status: "closed" };
    const log = threeWayDiff(anc, a, b);

    const out = poStates().merge(log.changes[0]!, ctx());
    expect(out).toEqual({ kind: "Resolved", value: "closed" });
  });

  it("diverging_transitions_escalate", () => {
    const anc = { status: "open" };
    const a = { status: "closed" };
    const b = { status: "cancelled" };
    const log = threeWayDiff(anc, a, b);

    const out = poStates().merge(log.changes[0]!, ctx());
    expect(out.kind).toBe("Conflict");
    if (out.kind === "Conflict") {
      expect(out.reason).toContain("divergent");
    }
  });
});

// ---------------------------------------------------------------------------
// Extra TS-side cases not covered by Rust tests but useful for confidence
// ---------------------------------------------------------------------------

describe("StateMachine — additional behaviour", () => {
  it("name() is 'state_machine'", () => {
    expect(poStates().name()).toBe("state_machine");
  });

  it("non-string ancestor conflicts", () => {
    const change: FieldChange = {
      path: "status",
      oldValue: 7,
      newFromA: "open",
      newFromB: undefined,
      source: "a",
    };
    const out = poStates().merge(change, ctx());
    expect(out.kind).toBe("Conflict");
    if (out.kind === "Conflict") {
      expect(out.reason).toContain("ancestor is not a string");
    }
  });

  it("non-string side-A conflicts", () => {
    const change: FieldChange = {
      path: "status",
      oldValue: "draft",
      newFromA: 42,
      newFromB: undefined,
      source: "a",
    };
    const out = poStates().merge(change, ctx());
    expect(out.kind).toBe("Conflict");
    if (out.kind === "Conflict") {
      expect(out.reason).toContain("a is not a string");
    }
  });

  it("identity transition (from === to) is accepted", () => {
    // "draft" -> "draft" isn't explicitly declared but the `from == to`
    // short-circuit keeps no-op merges alive.
    const change: FieldChange = {
      path: "status",
      oldValue: "draft",
      newFromA: "draft",
      newFromB: undefined,
      source: "a",
    };
    const out = poStates().merge(change, ctx());
    expect(out).toEqual({ kind: "Resolved", value: "draft" });
  });
});

// ---------------------------------------------------------------------------
// Property test: StateMachine rejects any transition not in the allow-list.
// ---------------------------------------------------------------------------

describe("StateMachine — property tests", () => {
  it("rejects transitions not in the allow-list", () => {
    const states = ["draft", "open", "closed", "cancelled", "archived"] as const;
    const allowed: Array<readonly [string, string]> = [
      ["draft", "open"],
      ["open", "closed"],
      ["open", "cancelled"],
    ];
    const allowedKey = (from: string, to: string) => `${from}->${to}`;
    const allowedSet = new Set(allowed.map(([f, t]) => allowedKey(f, t)));

    const sm = new StateMachine(
      allowed.map(([f, t]) => new StateTransition(f, t)),
    );

    fc.assert(
      fc.property(
        fc.constantFrom(...states),
        fc.constantFrom(...states),
        (from, to) => {
          const change: FieldChange = {
            path: "status",
            oldValue: from,
            newFromA: to,
            newFromB: undefined,
            source: "a",
          };
          const out = sm.merge(change, ctx());
          const shouldAllow = from === to || allowedSet.has(allowedKey(from, to));
          if (shouldAllow) {
            expect(out).toEqual({ kind: "Resolved", value: to });
          } else {
            expect(out.kind).toBe("Conflict");
          }
        },
      ),
      { numRuns: 200 },
    );
  });

  it("divergent both-sides transitions always escalate", () => {
    const sm = poStates();
    const change: FieldChange = {
      path: "status",
      oldValue: "open",
      newFromA: "closed",
      newFromB: "cancelled",
      source: "both",
    };
    const out = sm.merge(change, ctx());
    expect(out.kind).toBe("Conflict");
  });
});
