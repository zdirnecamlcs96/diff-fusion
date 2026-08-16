/**
 * Source-of-truth patterns — expressed via the Tier-1 `SyncEngine` facade.
 *
 * The Rust companion `examples/source_of_truth.rs` uses the deprecated
 * `ConflictStrategy` hint attached to CIF field definitions. This TS port
 * demonstrates the same conceptual lesson — "each field has one authoritative
 * owner" — but using the modern policy stack (`OwnedBy`) that Rust points
 * to as the recommended replacement.
 *
 * Two patterns:
 *  1. **Per-field ownership** — different systems own different fields of
 *     the same entity. OwnedBy(system) on each path routes winning values
 *     to the canonical view.
 *  2. **Whole-entity one-way** — one system is the source of truth for the
 *     entire entity. `.oneWay()` makes sideA authoritative;
 *     target-side edits revert on the next cycle.
 *
 * Uses only the public surface (SyncEngine + policies + adapter). No
 * Orchestrator or PolicyMap imports — facade only.
 *
 * Run with:
 *   npx tsx examples/sourceOfTruth.ts
 */

import type { JsonValue } from "../src/domain/types.js";
import { idempotencyKey } from "../src/domain/idempotency.js";
import { TestMemoryAdapter } from "../src/adapters/testMemory.js";
import { OwnedBy } from "../src/application/policy/ownedBy.js";
import { SyncEngine } from "../src/drivers/syncEngine.js";

const ENTITY = "product";
const PRODUCT_ID = "INV-001";

async function main(): Promise<void> {
  console.log("==== diff-fusion · source-of-truth patterns (facade) ====\n");

  await pattern1_fieldLevelOwnership();
  console.log();
  await pattern2_wholeEntityOneWay();

  console.log("\nKey takeaways:");
  console.log(" 1. Per-field OwnedBy routes authority without a central mediator.");
  console.log(" 2. .oneWay() is the shortcut for whole-entity source-of-truth.");
  console.log(" 3. Non-owner edits revert on the next cycle — predictable behaviour.");
}

/**
 * Pattern 1: per-field ownership. `inventory` owns stock; `pricing` owns
 * price fields. Both systems can drift on fields they don't own; those
 * edits revert.
 */
async function pattern1_fieldLevelOwnership(): Promise<void> {
  console.log("-- Pattern 1: per-field ownership -------------------------");

  const inventory = new TestMemoryAdapter("inventory");
  const pricing = new TestMemoryAdapter("pricing");

  const starting: JsonValue = {
    product_id: PRODUCT_ID,
    stock_quantity: 100, // inventory's domain
    price: 29.99, // pricing's domain
    discount_percentage: 10.0, // pricing's domain
  };
  inventory.seed(ENTITY, PRODUCT_ID, starting);
  pricing.seed(ENTITY, PRODUCT_ID, starting);

  // Each side makes a mix of owned and non-owned edits. Only the owned
  // ones should stick.
  await update(inventory, {
    product_id: PRODUCT_ID,
    stock_quantity: 80, // owned by inventory — wins
    price: 24.99, // not owned — will revert
    discount_percentage: 10.0,
  });
  await update(pricing, {
    product_id: PRODUCT_ID,
    stock_quantity: 95, // not owned — will revert
    price: 34.99, // owned by pricing — wins
    discount_percentage: 15.0, // owned by pricing — wins
  });

  console.log("BEFORE sync:");
  await printSide(inventory, "  inventory");
  await printSide(pricing, "  pricing  ");

  const engine = SyncEngine.builder(inventory, pricing)
    .policy("stock_quantity", new OwnedBy("inventory"))
    .policy("price", new OwnedBy("pricing"))
    .policy("discount_percentage", new OwnedBy("pricing"))
    .seedAncestor(ENTITY, PRODUCT_ID, starting)
    .build();

  const outcome = await engine.sync(ENTITY, PRODUCT_ID);
  printOutcome(outcome);

  console.log("AFTER sync:");
  await printSide(inventory, "  inventory");
  await printSide(pricing, "  pricing  ");
  // Expected on both sides: stock=80 (inventory wins), price=34.99, discount=15 (pricing wins).
  // product_id is unchanged — no policy, no divergence.
}

/**
 * Pattern 2: whole-entity one-way. `catalog` is the single source of
 * truth; `cache` mirrors it. `.oneWay()` installs
 * `OwnedBy(sideA)` as the default policy — any field not explicitly
 * overridden is owned by the source.
 */
async function pattern2_wholeEntityOneWay(): Promise<void> {
  console.log("-- Pattern 2: whole-entity one-way sync -------------------");

  const catalog = new TestMemoryAdapter("catalog");
  const cache = new TestMemoryAdapter("cache");

  const starting: JsonValue = {
    product_id: "CAT-042",
    name: "Widget",
    description: "A useful widget",
  };
  catalog.seed(ENTITY, "CAT-042", starting);
  cache.seed(ENTITY, "CAT-042", starting);

  // Catalog updates the description (authoritative edit).
  await updateAt(catalog, "CAT-042", {
    product_id: "CAT-042",
    name: "Widget",
    description: "A very useful widget",
  });
  // Cache drifts — a rogue edit that must be reverted.
  await updateAt(cache, "CAT-042", {
    product_id: "CAT-042",
    name: "Wdgt", // typo introduced downstream
    description: "A useful widget",
  });

  console.log("BEFORE sync:");
  await printSideAt(catalog, "CAT-042", "  catalog");
  await printSideAt(cache, "CAT-042", "  cache  ");

  // `.oneWay()` sets OwnedBy(catalog.systemType()) as the default — catalog
  // becomes authoritative for every field.
  const engine = SyncEngine.builder(catalog, cache).oneWay().build();

  const outcome = await engine.sync(ENTITY, "CAT-042");
  printOutcome(outcome);

  console.log("AFTER sync:");
  await printSideAt(catalog, "CAT-042", "  catalog");
  await printSideAt(cache, "CAT-042", "  cache  ");
  // Both converge to catalog's state; cache's typo is reverted.
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function printOutcome(outcome: Awaited<ReturnType<SyncEngine["sync"]>>): void {
  switch (outcome.kind) {
    case "NoOp":
      console.log("Outcome: NoOp (nothing to reconcile).");
      break;
    case "Synced":
      console.log(
        `Outcome: Synced. Pushed to: ${JSON.stringify(outcome.pushedTo)}`,
      );
      break;
    case "Escalated":
      console.log(
        `Outcome: Escalated — ${outcome.conflicts.length} conflict(s):`,
      );
      for (const c of outcome.conflicts) {
        console.log(`   · ${c.path} — ${c.reason}`);
      }
      break;
    default: {
      const _exhaustive: never = outcome;
      throw new Error(`unreachable SyncOutcome: ${JSON.stringify(_exhaustive)}`);
    }
  }
}

async function update(
  port: TestMemoryAdapter,
  newValue: JsonValue,
): Promise<void> {
  return updateAt(port, PRODUCT_ID, newValue);
}

async function updateAt(
  port: TestMemoryAdapter,
  id: string,
  newValue: JsonValue,
): Promise<void> {
  const current = await port.findByCanonicalId(ENTITY, id);
  if (current === undefined) {
    throw new Error(`seeded entity ${ENTITY}/${id} not found`);
  }
  const ik = idempotencyKey(id, "upsert", newValue);
  await port.upsert(ENTITY, id, newValue, current.version, ik);
}

async function printSide(
  port: TestMemoryAdapter,
  label: string,
): Promise<void> {
  return printSideAt(port, PRODUCT_ID, label);
}

async function printSideAt(
  port: TestMemoryAdapter,
  id: string,
  label: string,
): Promise<void> {
  const ext = await port.findByCanonicalId(ENTITY, id);
  if (ext === undefined) throw new Error(`no entity on ${port.systemType()}`);
  const { canonical } = await port.fetch(ENTITY, ext);
  console.log(`${label} (${port.systemType()}): ${JSON.stringify(canonical)}`);
}

main().catch((e: unknown) => {
  console.error(e);
  process.exit(1);
});
