/**
 * Facade-level tests — what the PUBLIC user-facing API looks like.
 *
 * These tests use only `SyncEngine` and the minimum types a user must know
 * about (policies, invariants, their own adapter). They do NOT import
 * `Orchestrator`, `InMemoryAncestorStore`, `PolicyMap`, or internal types.
 * Any leak of those through the facade means another method is needed.
 *
 * Port of `tests/integration/sync_engine_facade_tests.rs`.
 */

import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../src/domain/types.js";
import { TestMemoryAdapter } from "../../src/adapters/testMemory.js";
import { Additive } from "../../src/application/policy/additive.js";
import { OwnedBy } from "../../src/application/policy/ownedBy.js";
import {
  StateMachine,
  StateTransition,
} from "../../src/application/policy/stateMachine.js";
import { SetByKey } from "../../src/application/policy/structural.js";
import type {
  Invariant,
  InvariantOutcome,
} from "../../src/application/policy/invariants.js";
import { SyncEngine } from "../../src/drivers/syncEngine.js";

const ENTITY = "purchase_order";
const ID = "PO-42";

function seedPair(): { erp: TestMemoryAdapter; inv: TestMemoryAdapter } {
  const erp = new TestMemoryAdapter("erp");
  const inv = new TestMemoryAdapter("inv");
  erp.seed(ENTITY, ID, { price: 20, qty_recv: 6 });
  inv.seed(ENTITY, ID, { price: 10, qty_recv: 7 });
  return { erp, inv };
}

describe("happy_path_uses_only_facade_and_defaults", () => {
  it("build → sync returns Synced with both sides pushed", async () => {
    const { erp, inv } = seedPair();
    const engine = SyncEngine.builder(erp, inv)
      .policy("price", new OwnedBy("erp"))
      .policy("qty_recv", new Additive())
      .build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Synced");
    if (outcome.kind === "Synced") {
      expect(outcome.pushedTo).toContain("erp");
      expect(outcome.pushedTo).toContain("inv");
    }
    expect(await engine.escalationDepth()).toBe(0);
  });
});

describe("preview_reports_without_writing", () => {
  it("preview then sync still works — preview had no side effects", async () => {
    const { erp, inv } = seedPair();
    const engine = SyncEngine.builder(erp, inv)
      .policy("price", new OwnedBy("erp"))
      .policy("qty_recv", new Additive())
      .build();

    const preview = await engine.preview(ENTITY, ID);
    expect(preview.wouldWrite).toBeDefined();
    expect(preview.conflicts).toHaveLength(0);

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Synced");
  });
});

// Tier-2 invariant exposure ------------------------------------------------

class NeverNegativeQty implements Invariant {
  name(): string {
    return "never_negative_qty";
  }
  check(_prev: JsonValue, candidate: JsonValue): InvariantOutcome {
    if (
      typeof candidate === "object" &&
      candidate !== null &&
      !Array.isArray(candidate)
    ) {
      const q = (candidate as { qty_recv?: JsonValue }).qty_recv;
      if (typeof q === "number" && q < 0) {
        return { kind: "Reject", reason: "qty_recv must not be negative" };
      }
    }
    return { kind: "Pass" };
  }
}

describe("invariant_rejection_escalates_through_facade", () => {
  it("Reject surfaces via SyncOutcome.Escalated + escalationDepth", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    erp.seed(ENTITY, ID, { qty_recv: 3 });
    inv.seed(ENTITY, ID, { qty_recv: 2 });

    const engine = SyncEngine.builder(erp, inv)
      .policy("qty_recv", new Additive())
      .invariant(new NeverNegativeQty())
      .seedAncestor(ENTITY, ID, { qty_recv: 10 })
      .build();

    // a delta = 3-10 = -7; b delta = 2-10 = -8; merged = 10-7-8 = -5 → Reject.
    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      expect(outcome.conflicts).toHaveLength(1);
      expect(outcome.conflicts[0]?.reason).toContain("never_negative_qty");
    }
    expect(await engine.escalationDepth()).toBe(1);
  });
});

describe("one_way_preset_is_a_single_call", () => {
  it(".oneWay() is the whole preset — no PolicyMap/OwnedBy import needed", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    erp.seed(ENTITY, ID, { price: 20 });
    inv.seed(ENTITY, ID, { price: 99 }); // target drift

    const engine = SyncEngine.builder(erp, inv).oneWay().build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Synced");
    expect(await engine.escalationDepth()).toBe(0);
  });
});

// Tier-3 SetByKey — collection merges through the full cycle ---------------

describe("set_by_key_merges_additions_from_both_sides", () => {
  it("both sides converge to the union of added line items", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");

    const ancestor: JsonValue = { items: [{ sku: "X", q: 1 }] };
    erp.seed(ENTITY, ID, {
      items: [{ sku: "X", q: 1 }, { sku: "Y", q: 2 }],
    });
    inv.seed(ENTITY, ID, {
      items: [{ sku: "X", q: 1 }, { sku: "Z", q: 3 }],
    });

    const engine = SyncEngine.builder(erp, inv)
      .policy("items", new SetByKey(["sku"], "sku", "sku"))
      .seedAncestor(ENTITY, ID, ancestor)
      .build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Synced");

    for (const [port, label] of [[erp, "erp"], [inv, "inv"]] as const) {
      const ext = (await port.findByCanonicalId(ENTITY, ID))!;
      const { canonical } = await port.fetch(ENTITY, ext);
      const items = (canonical as { items: Array<{ sku: string }> }).items;
      const skus = items.map((i) => i.sku);
      expect(skus, `${label} missing X`).toContain("X");
      expect(skus, `${label} missing Y`).toContain("Y");
      expect(skus, `${label} missing Z`).toContain("Z");
      expect(items, `${label} unexpected length`).toHaveLength(3);
    }
    expect(await engine.escalationDepth()).toBe(0);
  });
});

describe("set_by_key_escalates_when_same_element_diverges", () => {
  it("divergent edits on the same element escalate and freeze state", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");

    const ancestor: JsonValue = { items: [{ sku: "X", q: 1 }] };
    erp.seed(ENTITY, ID, { items: [{ sku: "X", q: 10 }] });
    inv.seed(ENTITY, ID, { items: [{ sku: "X", q: 20 }] });

    const engine = SyncEngine.builder(erp, inv)
      .policy("items", new SetByKey(["sku"], "sku", "sku"))
      .seedAncestor(ENTITY, ID, ancestor)
      .build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      expect(outcome.conflicts).toHaveLength(1);
      expect(outcome.conflicts[0]?.reason).toContain("set_by_key");
    }

    // Neither side was pushed — each keeps its divergent state.
    const erpExt = (await erp.findByCanonicalId(ENTITY, ID))!;
    const invExt = (await inv.findByCanonicalId(ENTITY, ID))!;
    const erpView = (await erp.fetch(ENTITY, erpExt)).canonical;
    const invView = (await inv.fetch(ENTITY, invExt)).canonical;
    expect(erpView).toEqual({ items: [{ sku: "X", q: 10 }] });
    expect(invView).toEqual({ items: [{ sku: "X", q: 20 }] });

    expect(await engine.escalationDepth()).toBe(1);
  });
});

describe("set_by_key_honors_unilateral_deletion", () => {
  it("an unopposed removal drops the element without escalating", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");

    const ancestor: JsonValue = { items: [{ sku: "X" }, { sku: "Y" }] };
    erp.seed(ENTITY, ID, { items: [{ sku: "X" }] });
    inv.seed(ENTITY, ID, { items: [{ sku: "X" }, { sku: "Y" }] });

    const engine = SyncEngine.builder(erp, inv)
      .policy("items", new SetByKey(["sku"], "sku", "sku"))
      .seedAncestor(ENTITY, ID, ancestor)
      .build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Synced");

    for (const [port, label] of [[erp, "erp"], [inv, "inv"]] as const) {
      const ext = (await port.findByCanonicalId(ENTITY, ID))!;
      const { canonical } = await port.fetch(ENTITY, ext);
      const items = (canonical as { items: Array<{ sku: string }> }).items;
      expect(items, `${label}: Y should be gone`).toHaveLength(1);
      expect(items[0]?.sku).toBe("X");
    }
    expect(await engine.escalationDepth()).toBe(0);
  });
});

// ConflictClass taxonomy ---------------------------------------------------

class CapAt100 implements Invariant {
  name(): string {
    return "cap_at_100";
  }
  check(_prev: JsonValue, candidate: JsonValue): InvariantOutcome {
    if (
      typeof candidate !== "object" ||
      candidate === null ||
      Array.isArray(candidate)
    ) {
      return { kind: "Pass" };
    }
    const obj = candidate as { [k: string]: JsonValue };
    const q = obj.qty;
    if (typeof q === "number" && q > 100) {
      return {
        kind: "Transform",
        value: { ...obj, qty: 100 },
      };
    }
    return { kind: "Pass" };
  }
}

describe("invariant_transform_propagates_through_facade_to_adapter_state", () => {
  it("adapters store the TRANSFORMED value, not the raw resolved one", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    erp.seed(ENTITY, ID, { qty: 120 });
    inv.seed(ENTITY, ID, { qty: 130 });

    const engine = SyncEngine.builder(erp, inv)
      .policy("qty", new Additive())
      .invariant(new CapAt100())
      .seedAncestor(ENTITY, ID, { qty: 100 })
      .build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Synced");

    for (const [port, label] of [[erp, "erp"], [inv, "inv"]] as const) {
      const ref = (await port.findByCanonicalId(ENTITY, ID))!;
      const { canonical } = await port.fetch(ENTITY, ref);
      expect(canonical, `${label} must reflect transformed value`).toEqual({
        qty: 100,
      });
    }
  });
});

describe("preview_surfaces_conflict_class", () => {
  it("preview returns the same class info as sync would", async () => {
    const { erp, inv } = seedPair();
    const engine = SyncEngine.builder(erp, inv).build();

    const preview = await engine.preview(ENTITY, ID);
    expect(preview.wouldWrite).toBeUndefined();
    expect(preview.conflicts.length).toBeGreaterThan(0);
    for (const c of preview.conflicts) {
      expect(c.class).toBe("NoPolicy");
    }
  });
});

describe("unregistered_path_surfaces_no_policy_class", () => {
  it("every unresolved conflict with no policy carries NoPolicy class", async () => {
    const { erp, inv } = seedPair();
    const engine = SyncEngine.builder(erp, inv).build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      for (const c of outcome.conflicts) {
        expect(c.class).toBe("NoPolicy");
      }
    }
  });
});

describe("policy_rejection_surfaces_policy_conflict_class", () => {
  it("StateMachine rejection bubbles up with class=PolicyConflict", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    erp.seed(ENTITY, ID, { status: "closed" });
    inv.seed(ENTITY, ID, { status: "draft" });

    const engine = SyncEngine.builder(erp, inv)
      .policy(
        "status",
        new StateMachine([new StateTransition("open", "closed")]),
      )
      .build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      expect(
        outcome.conflicts.some((c) => c.class === "PolicyConflict"),
      ).toBe(true);
    }
  });
});

class RejectAll implements Invariant {
  name(): string {
    return "reject_all";
  }
  check(_p: JsonValue, _c: JsonValue): InvariantOutcome {
    return { kind: "Reject", reason: "by design" };
  }
}

describe("invariant_rejection_surfaces_invariant_violation_class", () => {
  it("Invariant Reject bubbles up with class=InvariantViolation", async () => {
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    erp.seed(ENTITY, ID, { price: 15 });
    inv.seed(ENTITY, ID, { price: 10 });

    const engine = SyncEngine.builder(erp, inv)
      .policy("price", new OwnedBy("erp"))
      .invariant(new RejectAll())
      .seedAncestor(ENTITY, ID, { price: 10 })
      .build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      expect(outcome.conflicts).toHaveLength(1);
      expect(outcome.conflicts[0]?.class).toBe("InvariantViolation");
    }
  });
});

describe("unregistered_conflict_is_visible_without_knowing_inner_types", () => {
  it("FacadeConflict.path and .reason give the caller what they need", async () => {
    const { erp, inv } = seedPair();
    const engine = SyncEngine.builder(erp, inv).build();

    const outcome = await engine.sync(ENTITY, ID);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      expect(
        outcome.conflicts.some(
          (c) => c.path === "price" || c.path === "qty_recv",
        ),
      ).toBe(true);
    }
  });
});

// validateAgainstSchema — Tier-3 schema-driven validation catches
// policy/schema mismatches before the first cycle runs.

describe("validate_against_schema_passes_when_anchors_line_up", () => {
  it("returns ok when the element schema declares both anchors", () => {
    const schema: JsonValue = {
      cif_schema: {
        items: {
          type: "array",
          element: {
            externalId: { type: "string", anchor: "a" },
            internalId: { type: "string", anchor: "b" },
            sku: { type: "string" },
          },
        },
      },
    };
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    const result = SyncEngine.builder(erp, inv)
      .policy("items", new SetByKey(["sku"], "externalId", "internalId"))
      .validateAgainstSchema(schema);
    expect(result).toEqual({ ok: true });
  });
});

describe("validate_against_schema_fails_when_anchor_field_missing", () => {
  it("flags a missing anchor with the path prefix", () => {
    const schema: JsonValue = {
      cif_schema: {
        items: {
          type: "array",
          element: {
            // externalId declared, internalId missing
            externalId: { type: "string", anchor: "a" },
            sku: { type: "string" },
          },
        },
      },
    };
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    const result = SyncEngine.builder(erp, inv)
      .policy("items", new SetByKey(["sku"], "externalId", "internalId"))
      .validateAgainstSchema(schema);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors).toHaveLength(1);
      expect(result.errors[0]).toMatch(/^items: /);
      expect(result.errors[0]).toContain("b_anchor 'internalId'");
    }
  });
});

describe("validate_against_schema_fails_when_schema_omits_field_entirely", () => {
  it("flags a policy bound to a path the schema doesn't declare at all", () => {
    const schema: JsonValue = {
      cif_schema: {
        something_else: { type: "string" },
      },
    };
    const erp = new TestMemoryAdapter("erp");
    const inv = new TestMemoryAdapter("inv");
    const result = SyncEngine.builder(erp, inv)
      .policy("items", new SetByKey(["sku"], "externalId", "internalId"))
      .validateAgainstSchema(schema);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.some((e) => e.includes("no CIF schema declared"))).toBe(
        true,
      );
    }
  });
});
