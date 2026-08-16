/**
 * Two-way reconciliation end to end — via the `SyncEngine` facade.
 *
 * This example imports only the facade, policy types, and the adapter.
 * No `Orchestrator`, no `PolicyMap.withDefault(...)` ceremony — everything
 * internal stays internal.
 *
 * Run with:
 *   npx tsx examples/twoWaySync.ts
 */

import type { JsonValue } from "../src/domain/types.js";
import { idempotencyKey } from "../src/domain/idempotency.js";
import { TestMemoryAdapter } from "../src/adapters/testMemory.js";
import { Additive } from "../src/application/policy/additive.js";
import { OwnedBy } from "../src/application/policy/ownedBy.js";
import {
  StateMachine,
  StateTransition,
} from "../src/application/policy/stateMachine.js";
import { SyncEngine } from "../src/drivers/syncEngine.js";

const ENTITY = "purchase_order";
const PO_ID = "PO-42";

async function main(): Promise<void> {
  console.log("==== diff-fusion · two-way sync (facade) ====\n");

  // --------------------------------------------------------------------
  // Set up two systems. Users only touch the adapter type.
  // --------------------------------------------------------------------
  const erp = new TestMemoryAdapter("erp");
  const inv = new TestMemoryAdapter("inv");

  const starting: JsonValue = {
    price: 100,
    qty_recv: 5,
    status: "open",
  };
  erp.seed(ENTITY, PO_ID, starting);
  inv.seed(ENTITY, PO_ID, starting);

  // Simulate drift on both sides.
  await update(erp, { price: 120, qty_recv: 6, status: "closed" });
  await update(inv, { price: 999, qty_recv: 7, status: "closed" });

  console.log("BEFORE sync:");
  await printSide(erp, "  erp");
  await printSide(inv, "  inv");
  console.log();

  // --------------------------------------------------------------------
  // Build the engine. One chain of builder calls — no orchestrator
  // import, no internal types.
  // --------------------------------------------------------------------
  const engine = SyncEngine.builder(erp, inv)
    .policy("price", new OwnedBy("erp"))
    .policy("qty_recv", new Additive())
    .policy(
      "status",
      new StateMachine([
        new StateTransition("open", "closed"),
        new StateTransition("open", "cancelled"),
      ]),
    )
    .seedAncestor(ENTITY, PO_ID, starting)
    .build();

  // --------------------------------------------------------------------
  // Shadow run first (dry run) — the facade calls it `preview`.
  // --------------------------------------------------------------------
  const preview = await engine.preview(ENTITY, PO_ID);
  console.log("PREVIEW (no writes):");
  if (preview.wouldWrite !== undefined) {
    console.log(`  would write: ${JSON.stringify(preview.wouldWrite)}`);
  } else {
    console.log(
      `  would escalate — ${preview.conflicts.length} conflicts`,
    );
    for (const c of preview.conflicts) {
      console.log(`    · ${c.path} — ${c.reason}`);
    }
  }
  console.log();

  // --------------------------------------------------------------------
  // Real cycle.
  // --------------------------------------------------------------------
  const outcome = await engine.sync(ENTITY, PO_ID);
  switch (outcome.kind) {
    case "NoOp":
      console.log("Nothing to do.");
      break;
    case "Synced":
      console.log(`Synced. Pushed to: ${JSON.stringify(outcome.pushedTo)}`);
      break;
    case "Escalated":
      console.log(
        `Escalated — ${outcome.conflicts.length} conflict(s) queued:`,
      );
      for (const c of outcome.conflicts) {
        console.log(`  · ${c.path} — ${c.reason}`);
      }
      break;
    default: {
      const _exhaustive: never = outcome;
      throw new Error(`unreachable SyncOutcome: ${JSON.stringify(_exhaustive)}`);
    }
  }

  console.log("\nAFTER sync:");
  await printSide(erp, "  erp");
  await printSide(inv, "  inv");

  console.log(`\nEscalation queue depth: ${await engine.escalationDepth()}`);

  // Replay is a NoOp — ancestor advanced, no drift remains.
  const replay = await engine.sync(ENTITY, PO_ID);
  console.log(`Replay outcome: ${JSON.stringify(replay)}`);
}

/** Overwrite an adapter's current view (demo-only helper). */
async function update(
  port: TestMemoryAdapter,
  newValue: JsonValue,
): Promise<void> {
  const current = await port.findByCanonicalId(ENTITY, PO_ID);
  if (current === undefined) {
    throw new Error(`seeded entity ${ENTITY}/${PO_ID} not found`);
  }
  const ik = idempotencyKey(PO_ID, "upsert", newValue);
  await port.upsert(ENTITY, PO_ID, newValue, current.version, ik);
}

async function printSide(
  port: TestMemoryAdapter,
  label: string,
): Promise<void> {
  const ext = await port.findByCanonicalId(ENTITY, PO_ID);
  if (ext === undefined) throw new Error(`no entity on ${port.systemType()}`);
  const { canonical } = await port.fetch(ENTITY, ext);
  console.log(`${label} (${port.systemType()}): ${JSON.stringify(canonical)}`);
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
