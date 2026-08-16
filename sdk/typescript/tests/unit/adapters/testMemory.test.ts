import { describe, expect, it } from "vitest";
import { TestMemoryAdapter } from "../../../src/adapters/testMemory.js";
import { idempotencyKey } from "../../../src/domain/idempotency.js";
import { SyncError } from "../../../src/domain/error.js";
import type { JsonValue } from "../../../src/domain/types.js";

function key(payload: JsonValue): Uint8Array {
  return idempotencyKey("PO-1", "upsert", payload);
}

describe("adapters/testMemory", () => {
  it("find_by_canonical_id returns none when missing", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const r = await a.findByCanonicalId("purchase_order", "PO-1");
    expect(r).toBeUndefined();
  });

  it("upsert creates and find returns ref", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const payload: JsonValue = { total: 100 };
    const r = await a.upsert("purchase_order", "PO-1", payload, undefined, key(payload));
    expect(r.system).toBe("sys_a");
    expect(r.version).toBeDefined();

    const found = await a.findByCanonicalId("purchase_order", "PO-1");
    expect(found).toBeDefined();
    expect(found!.system).toBe(r.system);
    expect(found!.externalId).toBe(r.externalId);
    expect(found!.version).toBe(r.version);
  });

  it("repeat upsert with same idempotency key is noop", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const payload: JsonValue = { total: 100 };
    const k = key(payload);
    const r1 = await a.upsert("purchase_order", "PO-1", payload, undefined, k);
    const r2 = await a.upsert("purchase_order", "PO-1", payload, undefined, k);
    expect(r2.system).toBe(r1.system);
    expect(r2.externalId).toBe(r1.externalId);
    expect(r2.version).toBe(r1.version);
  });

  it("new idempotency key advances version", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const k1 = idempotencyKey("PO-1", "upsert", { total: 100 });
    const k2 = idempotencyKey("PO-1", "upsert", { total: 200 });
    const r1 = await a.upsert("purchase_order", "PO-1", { total: 100 }, undefined, k1);
    const r2 = await a.upsert(
      "purchase_order",
      "PO-1",
      { total: 200 },
      r1.version,
      k2,
    );
    expect(r2.version).not.toBe(r1.version);
  });

  it("stale version produces StaleWrite error", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const k1 = idempotencyKey("PO-1", "upsert", { total: 100 });
    const k2 = idempotencyKey("PO-1", "upsert", { total: 200 });
    await a.upsert("purchase_order", "PO-1", { total: 100 }, undefined, k1);
    await expect(
      a.upsert("purchase_order", "PO-1", { total: 200 }, "999", k2),
    ).rejects.toSatisfy(
      (e) => e instanceof SyncError && e.kind === "StaleWrite",
    );
  });

  it("expect_version on nonexistent record is stale", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const k = idempotencyKey("PO-1", "upsert", {});
    await expect(
      a.upsert("purchase_order", "PO-1", {}, "1", k),
    ).rejects.toSatisfy(
      (e) => e instanceof SyncError && e.kind === "StaleWrite",
    );
  });

  it("fetch roundtrips", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const k = idempotencyKey("PO-1", "upsert", { total: 100 });
    const r = await a.upsert("purchase_order", "PO-1", { total: 100 }, undefined, k);
    const { canonical, ref } = await a.fetch("purchase_order", r);
    expect(canonical).toEqual({ total: 100 });
    expect(ref.externalId).toBe(r.externalId);
  });

  it("seeded entities are findable", async () => {
    const a = new TestMemoryAdapter("sys_a");
    const r = a.seed("purchase_order", "PO-7", { total: 77 });
    const found = await a.findByCanonicalId("purchase_order", "PO-7");
    expect(found).toBeDefined();
    expect(found!.system).toBe(r.system);
    expect(found!.externalId).toBe(r.externalId);
    expect(found!.version).toBe(r.version);
  });
});
