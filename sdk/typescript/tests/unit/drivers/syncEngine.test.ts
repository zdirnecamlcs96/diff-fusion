import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../../src/domain/types.js";
import { TestMemoryAdapter } from "../../../src/adapters/testMemory.js";
import { Additive } from "../../../src/application/policy/additive.js";
import { OwnedBy } from "../../../src/application/policy/ownedBy.js";
import {
  SyncEngine,
  SyncEngineBuilder,
  type FacadeConflict,
  type SyncOutcome,
} from "../../../src/drivers/syncEngine.js";
import type {
  FieldChange,
  MergeContext,
  MergeOutcome,
  MergePolicy,
} from "../../../src/application/policy/index.js";

// --- Unit-level smoke tests matching the Rust inline `#[tokio::test]` trio ---

describe("smoke_builds_and_runs", () => {
  it("build + sync returns a Synced outcome", async () => {
    const a = new TestMemoryAdapter("erp");
    const b = new TestMemoryAdapter("inv");
    a.seed("e", "1", { q: 11 });
    b.seed("e", "1", { q: 12 });

    const engine = SyncEngine.builder(a, b)
      .policy("q", new Additive())
      .seedAncestor("e", "1", { q: 10 })
      .build();

    const out = await engine.sync("e", "1");
    expect(out.kind).toBe("Synced");
  });
});

describe("one_way_shortcut_sets_owned_by_default", () => {
  it(".oneWay() installs OwnedBy(sideA.systemType()) as default", async () => {
    const a = new TestMemoryAdapter("source");
    const b = new TestMemoryAdapter("target");
    a.seed("e", "1", { x: 42 });
    b.seed("e", "1", { x: 99 });

    const engine = SyncEngine.builder(a, b).oneWay().build();
    const out = await engine.sync("e", "1");
    expect(out.kind).toBe("Synced");
  });
});

// --- Builder chaining / return type contract -------------------------------

describe("builder API", () => {
  it("every builder method returns the builder (fluent)", () => {
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    const builder = SyncEngine.builder(a, b);
    expect(builder).toBeInstanceOf(SyncEngineBuilder);

    const chained = builder
      .policy("x", new OwnedBy("a"))
      .oneWay()
      .seedAncestor("e", "1", { v: 1 });
    expect(chained).toBe(builder);
  });

  it("build() returns a SyncEngine even with no policies or seeds", () => {
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    const engine = SyncEngine.builder(a, b).build();
    expect(engine).toBeInstanceOf(SyncEngine);
  });
});

// --- Outcome flattening: no internal types leak ----------------------------

describe("outcome flattening", () => {
  it("SyncOutcome has exactly three kinds: NoOp | Synced | Escalated", async () => {
    // NoOp: identical views + matching ancestor.
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    a.seed("e", "1", { x: 1 });
    b.seed("e", "1", { x: 1 });
    const engine = SyncEngine.builder(a, b)
      .seedAncestor("e", "1", { x: 1 })
      .build();
    const out = await engine.sync("e", "1");
    expect(out.kind).toBe("NoOp");

    // Exhaustive switch ensures every kind is handled at the type level.
    const cases: SyncOutcome[] = [
      { kind: "NoOp" },
      { kind: "Synced", pushedTo: ["a"] },
      { kind: "Escalated", conflicts: [] },
    ];
    for (const c of cases) {
      switch (c.kind) {
        case "NoOp":
          break;
        case "Synced":
          expect(c.pushedTo).toBeInstanceOf(Array);
          break;
        case "Escalated":
          expect(c.conflicts).toBeInstanceOf(Array);
          break;
        default: {
          const _exhaustive: never = c;
          throw new Error(`unreachable: ${JSON.stringify(_exhaustive)}`);
        }
      }
    }
  });

  it("FacadeConflict exposes only { path, reason, class }", async () => {
    // Trigger a NoPolicy conflict via unregistered path.
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    a.seed("e", "1", { x: 2 });
    b.seed("e", "1", { x: 3 });
    const engine = SyncEngine.builder(a, b)
      .seedAncestor("e", "1", { x: 1 })
      .build();
    const out = await engine.sync("e", "1");
    expect(out.kind).toBe("Escalated");
    if (out.kind === "Escalated") {
      const c: FacadeConflict = out.conflicts[0]!;
      // Hard-assert the public surface: no `change`, no `FieldChange`, no
      // `UnresolvedConflict`-only fields leak.
      expect(Object.keys(c).sort()).toEqual(["class", "path", "reason"]);
      expect(c.class).toBe("NoPolicy");
    }
  });
});

// --- Custom ancestor / escalation injection --------------------------------

describe("custom store + queue wiring", () => {
  class RecordingStore {
    readonly entries = new Map<string, JsonValue>();
    async get(): Promise<undefined> {
      return undefined;
    }
    async put(
      key: { entityType: string; canonicalId: string },
      entry: { canonical: JsonValue },
    ): Promise<void> {
      this.entries.set(`${key.entityType}/${key.canonicalId}`, entry.canonical);
    }
    async delete(): Promise<void> {}
  }

  class RecordingQueue {
    readonly items: unknown[] = [];
    async push(item: unknown): Promise<void> {
      this.items.push(item);
    }
    async len(): Promise<number> {
      return this.items.length;
    }
    async isEmpty(): Promise<boolean> {
      return this.items.length === 0;
    }
  }

  it("custom ancestorStore overrides the in-memory default", async () => {
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    a.seed("e", "1", { x: 1 });
    b.seed("e", "1", { x: 1 });
    const store = new RecordingStore();
    const engine = SyncEngine.builder(a, b)
      .ancestorStore(store)
      .policy("x", new OwnedBy("a"))
      .build();
    await engine.sync("e", "1");
    // Cycle was NoOp (views match + bootstrap ancestor == viewA), so store
    // received no put. Instead, force a non-NoOp scenario.
    b.seed("e", "2", { x: 99 });
    a.seed("e", "2", { x: 1 });
    await engine.sync("e", "2");
    expect(store.entries.has("e/2")).toBe(true);
  });

  it("custom escalationQueue receives unresolved conflicts", async () => {
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    a.seed("e", "1", { price: 20 });
    b.seed("e", "1", { price: 10 });
    const q = new RecordingQueue();
    const engine = SyncEngine.builder(a, b)
      .escalationQueue(q)
      .seedAncestor("e", "1", { price: 5 })
      .build();
    const out = await engine.sync("e", "1");
    expect(out.kind).toBe("Escalated");
    expect(q.items).toHaveLength(1);
  });
});

// --- validateAgainstSchema without SetByKey -------------------------------

describe("validateAgainstSchema", () => {
  class DemandsElementSchema implements MergePolicy {
    name(): string {
      return "demands_element";
    }
    merge(_c: FieldChange, _ctx: MergeContext): MergeOutcome {
      return { kind: "Conflict", reason: "unused in these tests" };
    }
    validateAgainstSchema(fieldSchema: JsonValue): string[] {
      if (
        fieldSchema === null ||
        typeof fieldSchema !== "object" ||
        Array.isArray(fieldSchema) ||
        !("element" in fieldSchema)
      ) {
        return ["expected `element` on the schema for this path"];
      }
      return [];
    }
  }

  it("returns ok:true when every policy agrees with the schema", () => {
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    const schema: JsonValue = {
      cif_schema: {
        items: {
          type: "array",
          element: { sku: { type: "string" } },
        },
      },
    };
    const r = SyncEngine.builder(a, b)
      .policy("items", new DemandsElementSchema())
      .validateAgainstSchema(schema);
    expect(r).toEqual({ ok: true });
  });

  it("returns ok:false with path-prefixed errors on mismatch", () => {
    const a = new TestMemoryAdapter("a");
    const b = new TestMemoryAdapter("b");
    const schema: JsonValue = {
      cif_schema: {
        // no element key → demands_element policy rejects
        items: { type: "array" },
      },
    };
    const r = SyncEngine.builder(a, b)
      .policy("items", new DemandsElementSchema())
      .validateAgainstSchema(schema);
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.errors).toHaveLength(1);
      expect(r.errors[0]).toMatch(/^items:/);
    }
  });
});
