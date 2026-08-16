/**
 * End-to-end integration tests for the full sync cycle.
 *
 * Port of `tests/integration/sync_cycle_tests.rs`. Every Rust `#[tokio::test]`
 * maps to a vitest test of the same name, verifying the non-negotiable
 * behaviours from App.md § 05:
 *
 * - Ancestor advances only after both pushes confirm.
 * - An idempotent re-push does not create a duplicate or bump the version.
 * - Genuine conflicts land in the escalation queue and do not silently resolve.
 * - Empty changelogs do nothing.
 */

import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../src/domain/types.js";
import {
  AncestorEntry,
  AncestorKey,
} from "../../src/ports/ancestor.js";
import { InMemoryAncestorStore } from "../../src/adapters/inMemoryAncestor.js";
import { InMemoryEscalationQueue } from "../../src/adapters/inMemoryEscalation.js";
import { TestMemoryAdapter } from "../../src/adapters/testMemory.js";
import { PolicyMap } from "../../src/application/policy/index.js";
import { Additive } from "../../src/application/policy/additive.js";
import { OwnedBy } from "../../src/application/policy/ownedBy.js";
import {
  type Invariant,
  type InvariantOutcome,
  InvariantSet,
} from "../../src/application/policy/invariants.js";
import { Orchestrator } from "../../src/application/orchestrator.js";

const ENTITY = "purchase_order";
const ID = "PO-1";
const NOW = 1_700_000_000_000;

function buildOrchestrator(policies: PolicyMap) {
  const sideA = new TestMemoryAdapter("sys_a");
  const sideB = new TestMemoryAdapter("sys_b");
  const ancestor = new InMemoryAncestorStore();
  const escalation = new InMemoryEscalationQueue();
  const orch = new Orchestrator(sideA, sideB, ancestor, policies, escalation);
  return { orch, ancestor, escalation, sideA, sideB };
}

async function viewOf(
  side: TestMemoryAdapter,
  entityType: string,
  id: string,
): Promise<JsonValue> {
  const ref = await side.findByCanonicalId(entityType, id);
  if (ref === undefined) throw new Error(`no ref for ${entityType}/${id}`);
  const { canonical } = await side.fetch(entityType, ref);
  return canonical;
}

describe("identical_views_are_noop", () => {
  it("returns NoOp when neither side moved from ancestor", async () => {
    const policies = new PolicyMap().with("qty", new Additive());
    const { orch, ancestor, escalation, sideA, sideB } =
      buildOrchestrator(policies);

    sideA.seed(ENTITY, ID, { qty: 5 });
    sideB.seed(ENTITY, ID, { qty: 5 });
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ qty: 5 }, NOW - 1),
    );

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome).toEqual({ kind: "NoOp" });
    expect(await escalation.len()).toBe(0);
  });
});

describe("owned_field_propagates_one_way", () => {
  it("owner's change reaches non-owner; ancestor advances", async () => {
    const policies = new PolicyMap().with("price", new OwnedBy("sys_a"));
    const { orch, ancestor, escalation, sideA, sideB } =
      buildOrchestrator(policies);

    sideA.seed(ENTITY, ID, { price: 15 });
    sideB.seed(ENTITY, ID, { price: 10 });
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ price: 10 }, NOW - 1),
    );

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome.kind).toBe("Synced");
    if (outcome.kind === "Synced") {
      expect(outcome.pushedTo).toEqual(["sys_b"]);
    }

    // Ancestor advanced.
    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ price: 15 });
    expect(anc?.updatedAtMs).toBe(NOW);

    // B now reflects A's value.
    expect(await viewOf(sideB, ENTITY, ID)).toEqual({ price: 15 });

    expect(await escalation.len()).toBe(0);
  });
});

describe("additive_counters_accumulate_and_push_both_sides", () => {
  it("both deltas accumulate; both sides stale so both pushed", async () => {
    const policies = new PolicyMap().with("qty", new Additive());
    const { orch, ancestor, escalation, sideA, sideB } =
      buildOrchestrator(policies);

    sideA.seed(ENTITY, ID, { qty: 13 }); // +3 on A
    sideB.seed(ENTITY, ID, { qty: 12 }); // +2 on B
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ qty: 10 }, NOW - 1),
    );

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome.kind).toBe("Synced");
    if (outcome.kind === "Synced") {
      expect(outcome.pushedTo).toHaveLength(2);
      expect(outcome.pushedTo).toContain("sys_a");
      expect(outcome.pushedTo).toContain("sys_b");
    }

    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ qty: 15 });

    expect(await escalation.len()).toBe(0);
  });
});

describe("unresolvable_conflict_routes_to_escalation_and_blocks_writes", () => {
  it("escalates, leaves ancestor frozen, does not push", async () => {
    const policies = new PolicyMap();
    const { orch, ancestor, escalation, sideA, sideB } =
      buildOrchestrator(policies);

    sideA.seed(ENTITY, ID, { price: 15 });
    sideB.seed(ENTITY, ID, { price: 20 });
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ price: 10 }, NOW - 1),
    );

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      expect(outcome.conflicts).toHaveLength(1);
      expect(outcome.conflicts[0]?.path).toBe("price");
    }

    // Ancestor unchanged.
    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ price: 10 });
    expect(anc?.updatedAtMs).toBe(NOW - 1);

    // Escalation received the item.
    expect(await escalation.len()).toBe(1);
    const snap = escalation.snapshot();
    expect(snap[0]?.entityType).toBe(ENTITY);
    expect(snap[0]?.canonicalId).toBe(ID);

    // Neither side's state changed.
    expect(await viewOf(sideA, ENTITY, ID)).toEqual({ price: 15 });
    expect(await viewOf(sideB, ENTITY, ID)).toEqual({ price: 20 });
  });
});

describe("replayed_cycle_is_idempotent", () => {
  it("second run sees ancestor caught up → NoOp; no duplicate writes", async () => {
    const policies = new PolicyMap().with("qty", new Additive());
    const { orch, escalation, sideA, sideB } = buildOrchestrator(policies);

    sideA.seed(ENTITY, ID, { qty: 13 });
    sideB.seed(ENTITY, ID, { qty: 12 });

    const first = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(first.kind).toBe("Synced");

    const second = await orch.runCycleAt(ENTITY, ID, NOW + 1);
    expect(second).toEqual({ kind: "NoOp" });
    expect(await escalation.len()).toBe(0);
  });
});

describe("one_way_mode_propagates_source_and_reverts_target_drift", () => {
  it("source change propagates; target-side drift reverts; nothing escalated", async () => {
    const sideA = new TestMemoryAdapter("sys_a");
    const sideB = new TestMemoryAdapter("sys_b");
    const ancestor = new InMemoryAncestorStore();
    const escalation = new InMemoryEscalationQueue();

    sideA.seed(ENTITY, ID, { price: 20 }); // source moved
    sideB.seed(ENTITY, ID, { price: 99 }); // target drifted
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ price: 10 }, NOW - 1),
    );

    const orch = Orchestrator.oneWay(sideA, sideB, ancestor, escalation);

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome.kind).toBe("Synced");
    if (outcome.kind === "Synced") {
      expect(outcome.pushedTo).toContain("sys_b");
    }

    expect(await viewOf(sideA, ENTITY, ID)).toEqual({ price: 20 });
    expect(await viewOf(sideB, ENTITY, ID)).toEqual({ price: 20 });
    expect(await escalation.len()).toBe(0);
  });
});

// Tier-2 invariants -----------------------------------------------------

class RejectIfMarked implements Invariant {
  name(): string {
    return "reject_if_marked";
  }
  check(_previous: JsonValue, candidate: JsonValue): InvariantOutcome {
    if (
      typeof candidate === "object" &&
      candidate !== null &&
      !Array.isArray(candidate) &&
      (candidate as { must_fail?: JsonValue }).must_fail === true
    ) {
      return { kind: "Reject", reason: "candidate is marked must_fail" };
    }
    return { kind: "Pass" };
  }
}

describe("invariant_reject_blocks_pushes_and_escalates", () => {
  it("Tier-2 Reject does not push, does not advance ancestor", async () => {
    const sideA = new TestMemoryAdapter("sys_a");
    const sideB = new TestMemoryAdapter("sys_b");
    const ancestor = new InMemoryAncestorStore();
    const escalation = new InMemoryEscalationQueue();

    sideA.seed(ENTITY, ID, { must_fail: true, other: "a-val" });
    sideB.seed(ENTITY, ID, { must_fail: false, other: "b-val" });
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ must_fail: false, other: "anc-val" }, NOW - 1),
    );

    const policies = new PolicyMap().withDefault(new OwnedBy("sys_a"));
    const invariants = new InvariantSet().with(new RejectIfMarked());

    const orch = new Orchestrator(
      sideA,
      sideB,
      ancestor,
      policies,
      escalation,
    ).withInvariants(invariants);

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome.kind).toBe("Escalated");
    if (outcome.kind === "Escalated") {
      expect(outcome.conflicts).toHaveLength(1);
      expect(outcome.conflicts[0]?.reason).toContain("reject_if_marked");
    }

    expect(await escalation.len()).toBe(1);

    // Ancestor did NOT advance.
    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ must_fail: false, other: "anc-val" });
    expect(anc?.updatedAtMs).toBe(NOW - 1);

    // Side B did NOT receive the poisoned value.
    expect(await viewOf(sideB, ENTITY, ID)).toEqual({
      must_fail: false,
      other: "b-val",
    });
  });
});

class CapQtyAt100 implements Invariant {
  name(): string {
    return "cap_qty_at_100";
  }
  check(_previous: JsonValue, candidate: JsonValue): InvariantOutcome {
    if (
      typeof candidate !== "object" ||
      candidate === null ||
      Array.isArray(candidate)
    ) {
      return { kind: "Pass" };
    }
    const q = (candidate as { qty?: JsonValue }).qty;
    if (typeof q === "number" && q > 100) {
      return {
        kind: "Transform",
        value: { ...(candidate as { [k: string]: JsonValue }), qty: 100 },
      };
    }
    return { kind: "Pass" };
  }
}

describe("invariant_transform_rewrites_pushed_value", () => {
  it("Transform replaces candidate before push; ancestor stores transformed", async () => {
    const sideA = new TestMemoryAdapter("sys_a");
    const sideB = new TestMemoryAdapter("sys_b");
    const ancestor = new InMemoryAncestorStore();
    const escalation = new InMemoryEscalationQueue();

    sideA.seed(ENTITY, ID, { qty: 120 });
    sideB.seed(ENTITY, ID, { qty: 130 });
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ qty: 100 }, NOW - 1),
    );

    const policies = new PolicyMap().with("qty", new Additive());
    const invariants = new InvariantSet().with(new CapQtyAt100());

    const orch = new Orchestrator(
      sideA,
      sideB,
      ancestor,
      policies,
      escalation,
    ).withInvariants(invariants);

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome.kind).toBe("Synced");

    expect(await viewOf(sideA, ENTITY, ID)).toEqual({ qty: 100 });
    expect(await viewOf(sideB, ENTITY, ID)).toEqual({ qty: 100 });

    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ qty: 100 });

    expect(await escalation.len()).toBe(0);
  });
});

class AlwaysPass implements Invariant {
  name(): string {
    return "always_pass";
  }
  check(_previous: JsonValue, _candidate: JsonValue): InvariantOutcome {
    return { kind: "Pass" };
  }
}

describe("invariant_pass_is_a_noop", () => {
  it("Pass invariant leaves baseline behaviour intact", async () => {
    const sideA = new TestMemoryAdapter("sys_a");
    const sideB = new TestMemoryAdapter("sys_b");
    const ancestor = new InMemoryAncestorStore();
    const escalation = new InMemoryEscalationQueue();

    sideA.seed(ENTITY, ID, { price: 15 });
    sideB.seed(ENTITY, ID, { price: 10 });
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ price: 10 }, NOW - 1),
    );

    const policies = new PolicyMap().with("price", new OwnedBy("sys_a"));
    const invariants = new InvariantSet().with(new AlwaysPass());

    const orch = new Orchestrator(
      sideA,
      sideB,
      ancestor,
      policies,
      escalation,
    ).withInvariants(invariants);

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome.kind).toBe("Synced");
    if (outcome.kind === "Synced") {
      expect(outcome.pushedTo).toEqual(["sys_b"]);
    }

    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ price: 15 });
    expect(anc?.updatedAtMs).toBe(NOW);

    expect(await viewOf(sideB, ENTITY, ID)).toEqual({ price: 15 });

    expect(await escalation.len()).toBe(0);
  });
});

describe("shadow_mode_reports_without_writing", () => {
  it("runShadow produces a report but never touches adapters or ancestor", async () => {
    const policies = new PolicyMap().with("price", new OwnedBy("sys_a"));
    const { orch, ancestor, escalation, sideA, sideB } =
      buildOrchestrator(policies);

    sideA.seed(ENTITY, ID, { price: 15 });
    sideB.seed(ENTITY, ID, { price: 10 });
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ price: 10 }, NOW - 1),
    );

    const report = await orch.runShadow(ENTITY, ID);
    expect(report.changelog.changes.length).toBeGreaterThan(0);
    expect(report.resolution.isClean()).toBe(true);
    expect(report.wouldWrite).toEqual({ price: 15 });

    // Nothing changed externally.
    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ price: 10 });
    expect(anc?.updatedAtMs).toBe(NOW - 1);
    expect(await escalation.len()).toBe(0);

    expect(await viewOf(sideB, ENTITY, ID)).toEqual({ price: 10 });
  });
});

describe("bootstrap: missing ancestor uses side-A view as baseline", () => {
  it("first cycle with no ancestor treats A as the baseline", async () => {
    // A and B both carry the same value; no ancestor stored. The orchestrator
    // should see an empty changelog (A == bootstrapped-A, B == A) → NoOp.
    const policies = new PolicyMap();
    const { orch, ancestor, sideA, sideB } = buildOrchestrator(policies);

    sideA.seed(ENTITY, ID, { price: 10 });
    sideB.seed(ENTITY, ID, { price: 10 });

    const outcome = await orch.runCycleAt(ENTITY, ID, NOW);
    expect(outcome).toEqual({ kind: "NoOp" });
    // Ancestor was NOT written (bootstrap only happens inside the cycle for
    // diff purposes — NoOp short-circuits before commit).
    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc).toBeUndefined();
  });
});
