/**
 * Partial push failure — the ancestor must stay frozen when any push fails.
 *
 * Pins down the single most load-bearing ordering rule in the library (plan
 * §Phase 6 step 7): the ancestor advances only after ALL pushes confirm.
 * If side A's push succeeds but side B's fails mid-sequence, the ancestor
 * must not move, so the next cycle re-derives everything from current
 * state. Side A will still carry its write — that's the honest consequence
 * of sequential (non-two-phase-commit) pushes. Replay is the recovery.
 */

import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../src/domain/types.js";
import { SyncError } from "../../src/domain/error.js";
import { TestMemoryAdapter } from "../../src/adapters/testMemory.js";
import { InMemoryAncestorStore } from "../../src/adapters/inMemoryAncestor.js";
import { InMemoryEscalationQueue } from "../../src/adapters/inMemoryEscalation.js";
import {
  AncestorEntry,
  AncestorKey,
} from "../../src/ports/ancestor.js";
import {
  type ExternalRef,
  type SystemPort,
} from "../../src/ports/system.js";
import { PolicyMap } from "../../src/application/policy/index.js";
import { Additive } from "../../src/application/policy/additive.js";
import { OwnedBy } from "../../src/application/policy/ownedBy.js";
import { Orchestrator } from "../../src/application/orchestrator.js";

const ENTITY = "purchase_order";
const ID = "PO-1";
const NOW = 1_700_000_000_000;

/**
 * Adapter wrapper that delegates to `inner` for fetch / findByCanonicalId,
 * but forces every `upsert` to throw `SyncError.staleWrite` until
 * `allowUpsert()` flips it into passthrough mode.
 */
class FailingUpsertAdapter implements SystemPort {
  private failing = true;

  constructor(private readonly inner: TestMemoryAdapter) {}

  allowUpsert(): void {
    this.failing = false;
  }

  systemType(): string {
    return this.inner.systemType();
  }
  async fetch(entityType: string, ext: ExternalRef) {
    return this.inner.fetch(entityType, ext);
  }
  async findByCanonicalId(entityType: string, canonicalId: string) {
    return this.inner.findByCanonicalId(entityType, canonicalId);
  }
  async upsert(
    entityType: string,
    canonicalId: string,
    canonical: JsonValue,
    expectVersion: string | undefined,
    idempotencyKey: Uint8Array,
  ): Promise<ExternalRef> {
    if (this.failing) {
      throw SyncError.staleWrite({
        system: this.inner.systemType(),
        message: "FailingUpsertAdapter: upsert forced to fail",
        expected: "forced",
      });
    }
    return this.inner.upsert(
      entityType,
      canonicalId,
      canonical,
      expectVersion,
      idempotencyKey,
    );
  }
}

async function viewOf(
  side: TestMemoryAdapter,
  entityType: string,
  id: string,
): Promise<JsonValue> {
  const ref = (await side.findByCanonicalId(entityType, id))!;
  const { canonical } = await side.fetch(entityType, ref);
  return canonical;
}

describe("partial_failure_leaves_ancestor_unchanged", () => {
  it("StaleWrite surfaces; ancestor FROZEN — the most important invariant", async () => {
    const innerA = new TestMemoryAdapter("sys_a");
    const innerB = new TestMemoryAdapter("sys_b");
    innerA.seed(ENTITY, ID, { price: 15 });
    innerB.seed(ENTITY, ID, { price: 10 });

    const sideB = new FailingUpsertAdapter(innerB);

    const ancestor = new InMemoryAncestorStore();
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ price: 10 }, NOW - 1),
    );
    const escalation = new InMemoryEscalationQueue();

    const policies = new PolicyMap().with("price", new OwnedBy("sys_a"));
    const orch = new Orchestrator(
      innerA,
      sideB,
      ancestor,
      policies,
      escalation,
    );

    // (1) StaleWrite surfaces to the caller.
    await expect(orch.runCycleAt(ENTITY, ID, NOW)).rejects.toMatchObject({
      kind: "StaleWrite",
    });

    // (2) Ancestor is FROZEN — the single most important invariant.
    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ price: 10 });
    expect(anc?.updatedAtMs).toBe(NOW - 1);
  });
});

describe("partial_failure_side_a_still_carries_its_write", () => {
  it("A pushes before B fails; A is updated, B retains pre-cycle state", async () => {
    const innerA = new TestMemoryAdapter("sys_a");
    const innerB = new TestMemoryAdapter("sys_b");
    innerA.seed(ENTITY, ID, { qty: 13 }); // +3
    innerB.seed(ENTITY, ID, { qty: 12 }); // +2
    const sideB = new FailingUpsertAdapter(innerB);

    const ancestor = new InMemoryAncestorStore();
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ qty: 10 }, NOW - 1),
    );
    const escalation = new InMemoryEscalationQueue();

    const policies = new PolicyMap().with("qty", new Additive());
    const orch = new Orchestrator(
      innerA,
      sideB,
      ancestor,
      policies,
      escalation,
    );

    // Push to A succeeds, push to B throws; orchestrator propagates the throw.
    await expect(orch.runCycleAt(ENTITY, ID, NOW)).rejects.toBeInstanceOf(
      SyncError,
    );

    // Merged value = 10 + 3 + 2 = 15. A got pushed first; B's push failed.
    expect(await viewOf(innerA, ENTITY, ID)).toEqual({ qty: 15 });
    expect(await viewOf(innerB, ENTITY, ID)).toEqual({ qty: 12 });
  });
});

describe("replay_after_partial_failure_converges_b", () => {
  it("after fixing B, replay pushes B; ancestor finally advances", async () => {
    const innerA = new TestMemoryAdapter("sys_a");
    const innerB = new TestMemoryAdapter("sys_b");
    innerA.seed(ENTITY, ID, { price: 15 });
    innerB.seed(ENTITY, ID, { price: 10 });

    const sideB = new FailingUpsertAdapter(innerB);

    const ancestor = new InMemoryAncestorStore();
    await ancestor.put(
      new AncestorKey(ENTITY, ID),
      new AncestorEntry({ price: 10 }, NOW - 1),
    );
    const escalation = new InMemoryEscalationQueue();

    const policies = new PolicyMap().with("price", new OwnedBy("sys_a"));
    const orch = new Orchestrator(
      innerA,
      sideB,
      ancestor,
      policies,
      escalation,
    );

    // First cycle: B fails, ancestor stays at 10.
    await expect(orch.runCycleAt(ENTITY, ID, NOW)).rejects.toMatchObject({
      kind: "StaleWrite",
    });

    // Fix side B and replay.
    sideB.allowUpsert();
    const second = await orch.runCycleAt(ENTITY, ID, NOW + 1);
    expect(second.kind).toBe("Synced");
    if (second.kind === "Synced") {
      expect(second.pushedTo).toContain("sys_b");
    }

    // B now has the merged value.
    expect(await viewOf(innerB, ENTITY, ID)).toEqual({ price: 15 });

    // Ancestor finally advanced — to the merged value, with replay timestamp.
    const anc = await ancestor.get(new AncestorKey(ENTITY, ID));
    expect(anc?.canonical).toEqual({ price: 15 });
    expect(anc?.updatedAtMs).toBe(NOW + 1);
  });
});
