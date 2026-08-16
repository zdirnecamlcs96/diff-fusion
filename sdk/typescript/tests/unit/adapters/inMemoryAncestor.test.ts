import { describe, expect, it } from "vitest";
import {
  AncestorEntry,
  AncestorKey,
} from "../../../src/ports/ancestor.js";
import { InMemoryAncestorStore } from "../../../src/adapters/inMemoryAncestor.js";

describe("put_then_get_roundtrips", () => {
  it("stores then returns the same AncestorEntry", async () => {
    const store = new InMemoryAncestorStore();
    const key = new AncestorKey("purchase_order", "PO-1");
    const entry = new AncestorEntry({ total: 100 }, 1_700_000_000_000);

    await store.put(key, entry);
    const got = await store.get(key);
    expect(got).toBeDefined();
    expect(got?.canonical).toEqual({ total: 100 });
    expect(got?.updatedAtMs).toBe(1_700_000_000_000);
  });
});

describe("get_returns_undefined_for_missing", () => {
  it("returns undefined (not null, not throw) for missing keys", async () => {
    const store = new InMemoryAncestorStore();
    const key = new AncestorKey("invoice", "INV-999");
    const got = await store.get(key);
    expect(got).toBeUndefined();
  });
});

describe("put_overwrites", () => {
  it("second put for the same key replaces the first", async () => {
    const store = new InMemoryAncestorStore();
    const key = new AncestorKey("item", "SKU-1");
    await store.put(key, new AncestorEntry({ v: 1 }, 1));
    await store.put(key, new AncestorEntry({ v: 2 }, 2));

    const got = await store.get(key);
    expect(got?.canonical).toEqual({ v: 2 });
    expect(got?.updatedAtMs).toBe(2);
  });
});

describe("different_entity_types_do_not_collide", () => {
  it("same canonicalId across different entityType stays separate", async () => {
    const store = new InMemoryAncestorStore();
    const k1 = new AncestorKey("purchase_order", "ID-1");
    const k2 = new AncestorKey("invoice", "ID-1");
    await store.put(k1, new AncestorEntry({ kind: "po" }, 1));
    await store.put(k2, new AncestorEntry({ kind: "inv" }, 2));

    expect((await store.get(k1))?.canonical).toEqual({ kind: "po" });
    expect((await store.get(k2))?.canonical).toEqual({ kind: "inv" });
  });
});

describe("key semantics", () => {
  it("key equality is by (entityType, canonicalId), not object identity", async () => {
    const store = new InMemoryAncestorStore();
    const k1 = new AncestorKey("x", "1");
    const k2 = new AncestorKey("x", "1");
    await store.put(k1, new AncestorEntry({ v: 1 }, 1));
    const got = await store.get(k2);
    expect(got?.canonical).toEqual({ v: 1 });
  });
});
