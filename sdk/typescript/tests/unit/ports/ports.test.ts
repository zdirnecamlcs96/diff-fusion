import { describe, expect, it } from "vitest";
import {
  AncestorEntry,
  AncestorKey,
  AncestorStoreError,
  type AncestorStore,
} from "../../../src/ports/ancestor.js";
import {
  EscalationError,
  EscalationItem,
  type EscalationQueue,
} from "../../../src/ports/escalation.js";
import { ExternalRef, type SystemPort } from "../../../src/ports/system.js";
import type { JsonValue } from "../../../src/domain/types.js";
import type { UnresolvedConflict } from "../../../src/application/policy/index.js";

// The Rust unit tests for ports are mostly struct-construction smoke checks.
// Ports in TS are interface-only, so the real guarantee is "tsc passes", which
// the test build gives us for free. The tests below pin the small runtime
// surface (default constructors, factories, error formatting) and fuzz-build a
// mock adapter to prove the interfaces are actually implementable.

describe("ports/ancestor", () => {
  it("AncestorKey holds its fields", () => {
    const k = new AncestorKey("PurchaseOrder", "PO-42");
    expect(k.entityType).toBe("PurchaseOrder");
    expect(k.canonicalId).toBe("PO-42");
  });

  it("AncestorEntry holds canonical + timestamp", () => {
    const canonical: JsonValue = { x: 1 };
    const e = new AncestorEntry(canonical, 12345);
    expect(e.canonical).toBe(canonical);
    expect(e.updatedAtMs).toBe(12345);
  });

  it("AncestorStoreError formats its message", () => {
    const err = new AncestorStoreError("disk full");
    expect(err.message).toBe("ancestor store backend failure: disk full");
    expect(err.name).toBe("AncestorStoreError");
    expect(err).toBeInstanceOf(Error);
  });

  it("AncestorStore interface is implementable with async methods", async () => {
    const store: AncestorStore = {
      async get() {
        return undefined;
      },
      async put() {},
    };
    await expect(
      store.get(new AncestorKey("T", "id")),
    ).resolves.toBeUndefined();
  });
});

describe("ports/escalation", () => {
  it("EscalationItem retains conflicts + metadata", () => {
    const conflicts: UnresolvedConflict[] = [];
    const item = new EscalationItem("Order", "ORD-1", conflicts, 42);
    expect(item.entityType).toBe("Order");
    expect(item.canonicalId).toBe("ORD-1");
    expect(item.conflicts).toBe(conflicts);
    expect(item.createdAtMs).toBe(42);
  });

  it("EscalationError formats its message", () => {
    const err = new EscalationError("out of capacity");
    expect(err.message).toBe(
      "escalation queue backend failure: out of capacity",
    );
    expect(err.name).toBe("EscalationError");
  });

  it("EscalationQueue interface is implementable with async methods", async () => {
    const items: EscalationItem[] = [];
    const queue: EscalationQueue = {
      async push(i) {
        items.push(i);
      },
      async len() {
        return items.length;
      },
      async isEmpty() {
        return items.length === 0;
      },
    };
    expect(await queue.isEmpty()).toBe(true);
    await queue.push(new EscalationItem("T", "id", [], 0));
    expect(await queue.len()).toBe(1);
    expect(await queue.isEmpty()).toBe(false);
  });
});

describe("ports/system", () => {
  it("ExternalRef holds its fields (version optional)", () => {
    const r = new ExternalRef("netsuite", "PO-42", "v7");
    expect(r.system).toBe("netsuite");
    expect(r.externalId).toBe("PO-42");
    expect(r.version).toBe("v7");

    const unversioned = new ExternalRef("internal", "id-1", undefined);
    expect(unversioned.version).toBeUndefined();
  });

  it("SystemPort interface is implementable with a minimal mock", async () => {
    const port: SystemPort = {
      systemType: () => "mock",
      async fetch(_entityType, ext) {
        return { canonical: { hello: "world" } as JsonValue, ref: ext };
      },
      async findByCanonicalId() {
        return undefined;
      },
      async upsert(_entityType, _canonicalId, _canonical, _expectVersion, _key) {
        return new ExternalRef("mock", "ext-1", "v1");
      },
    };

    expect(port.systemType()).toBe("mock");

    const ext = new ExternalRef("mock", "ext-1", undefined);
    const fetched = await port.fetch("T", ext);
    expect(fetched.canonical).toEqual({ hello: "world" });

    const upserted = await port.upsert(
      "T",
      "id",
      { x: 1 } as JsonValue,
      undefined,
      new Uint8Array(32),
    );
    expect(upserted.externalId).toBe("ext-1");
  });
});
