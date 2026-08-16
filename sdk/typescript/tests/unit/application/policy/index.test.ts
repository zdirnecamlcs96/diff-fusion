import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../../../src/domain/types.js";
import {
  type ChangeSource,
  type ConflictClass,
  type FieldChange,
  MergeContext,
  type MergeOutcome,
  type MergePolicy,
  PolicyMap,
  Resolution,
  type UnresolvedConflict,
  resolve,
} from "../../../../src/application/policy/index.js";

// Helper constructors to keep tests readable.
function change(
  path: string,
  old: JsonValue,
  a: JsonValue | undefined,
  b: JsonValue | undefined,
  source: ChangeSource,
): FieldChange {
  return {
    path,
    oldValue: old,
    newFromA: a,
    newFromB: b,
    source,
  };
}

class AlwaysResolveToA implements MergePolicy {
  name(): string {
    return "always_a";
  }
  merge(c: FieldChange, _ctx: MergeContext): MergeOutcome {
    const v = c.newFromA ?? c.oldValue;
    return { kind: "Resolved", value: v };
  }
}

class AlwaysConflict implements MergePolicy {
  name(): string {
    return "always_conflict";
  }
  merge(_c: FieldChange, _ctx: MergeContext): MergeOutcome {
    return { kind: "Conflict", reason: "by design" };
  }
}

describe("MergeContext", () => {
  it("stores system labels", () => {
    const ctx = new MergeContext("netsuite", "our_inventory");
    expect(ctx.system_a).toBe("netsuite");
    expect(ctx.system_b).toBe("our_inventory");
  });
});

describe("PolicyMap lookup", () => {
  it("returns registered policy by exact path", () => {
    const p = new AlwaysResolveToA();
    const map = new PolicyMap().with("x", p);
    expect(map.lookup("x")).toBe(p);
  });

  it("falls back to default when path unregistered", () => {
    const def = new AlwaysResolveToA();
    const map = new PolicyMap().withDefault(def);
    expect(map.lookup("y")).toBe(def);
  });

  it("explicit policy wins over default", () => {
    const def = new AlwaysResolveToA();
    const specific = new AlwaysConflict();
    const map = new PolicyMap().withDefault(def).with("y", specific);
    expect(map.lookup("y")).toBe(specific);
    expect(map.lookup("z")).toBe(def);
  });

  it("returns undefined when neither match", () => {
    const map = new PolicyMap();
    expect(map.lookup("anything")).toBeUndefined();
  });
});

describe("resolve()", () => {
  const ctx = new MergeContext("sys_a", "sys_b");

  it("unregistered_path_becomes_conflict", () => {
    const log = { changes: [change("x", 1, 2, undefined, "a")] };
    const r = resolve(log, new PolicyMap(), ctx);
    expect(r.resolved.size).toBe(0);
    expect(r.conflicts).toHaveLength(1);
    expect(r.conflicts[0]?.reason).toContain("no policy");
    expect(r.conflicts[0]?.class).toBe<ConflictClass>("NoPolicy");
    expect(r.conflicts[0]?.path).toBe("x");
  });

  it("default_policy_catches_unregistered", () => {
    const log = {
      changes: [
        change("x", 1, 10, undefined, "a"),
        change("y", 2, 20, undefined, "a"),
      ],
    };
    const policies = new PolicyMap().withDefault(new AlwaysResolveToA());
    const r = resolve(log, policies, ctx);
    expect(r.resolved.size).toBe(2);
    expect(r.resolved.get("x")).toBe(10);
    expect(r.resolved.get("y")).toBe(20);
    expect(r.conflicts).toHaveLength(0);
    expect(r.isClean()).toBe(true);
  });

  it("explicit_policy_overrides_default", () => {
    const log = {
      changes: [
        change("x", 1, 10, undefined, "a"),
        change("y", 2, 20, undefined, "a"),
      ],
    };
    const policies = new PolicyMap()
      .withDefault(new AlwaysResolveToA())
      .with("y", new AlwaysConflict());
    const r = resolve(log, policies, ctx);
    expect(r.resolved.size).toBe(1);
    expect(r.resolved.has("x")).toBe(true);
    expect(r.conflicts).toHaveLength(1);
    expect(r.conflicts[0]?.path).toBe("y");
    expect(r.conflicts[0]?.class).toBe<ConflictClass>("PolicyConflict");
  });

  it("conflict_reason_includes_policy_name", () => {
    const log = { changes: [change("x", 1, 2, undefined, "a")] };
    const policies = new PolicyMap().with("x", new AlwaysConflict());
    const r = resolve(log, policies, ctx);
    expect(r.conflicts[0]?.reason.startsWith("always_conflict:")).toBe(true);
  });

  it("preserves the original FieldChange in the conflict record", () => {
    const c = change("x", 1, 2, undefined, "a");
    const log = { changes: [c] };
    const r = resolve(log, new PolicyMap(), ctx);
    const conflict: UnresolvedConflict | undefined = r.conflicts[0];
    expect(conflict?.change.path).toBe("x");
    expect(conflict?.change.oldValue).toBe(1);
    expect(conflict?.change.newFromA).toBe(2);
    expect(conflict?.change.source).toBe("a");
  });

  it("returns a Resolution instance with isClean()", () => {
    const r = resolve({ changes: [] }, new PolicyMap(), ctx);
    expect(r).toBeInstanceOf(Resolution);
    expect(r.isClean()).toBe(true);
  });
});

describe("PolicyMap.validateAgainstSchema", () => {
  class RequiresElement implements MergePolicy {
    name(): string {
      return "requires_element";
    }
    merge(_c: FieldChange, _ctx: MergeContext): MergeOutcome {
      return { kind: "Conflict", reason: "unused" };
    }
    validateAgainstSchema(fieldSchema: JsonValue): string[] {
      if (
        fieldSchema === null ||
        typeof fieldSchema !== "object" ||
        Array.isArray(fieldSchema) ||
        !("element" in fieldSchema)
      ) {
        return ["expected `element`"];
      }
      return [];
    }
  }

  it("returns no errors when policies don't override validation (default empty)", () => {
    const map = new PolicyMap().with("x", new AlwaysResolveToA());
    const errors = map.validateAgainstSchema({ cif_schema: {} });
    expect(errors).toEqual([]);
  });

  it("prefixes errors with the policy path", () => {
    const map = new PolicyMap().with("items", new RequiresElement());
    const errors = map.validateAgainstSchema({ cif_schema: { items: {} } });
    expect(errors).toHaveLength(1);
    expect(errors[0]).toBe("items: expected `element`");
  });

  it("treats a missing path as null field_schema (matches Rust fallback)", () => {
    const map = new PolicyMap().with("ghost", new RequiresElement());
    const errors = map.validateAgainstSchema({ cif_schema: {} });
    expect(errors[0]).toBe("ghost: expected `element`");
  });
});

describe("MergeOutcome discriminated union", () => {
  it("exhaustive switches compile (kind tags are literal)", () => {
    const outcomes: MergeOutcome[] = [
      { kind: "Resolved", value: 1 },
      { kind: "Conflict", reason: "x" },
    ];
    for (const o of outcomes) {
      switch (o.kind) {
        case "Resolved":
          expect(o.value).toBe(1);
          break;
        case "Conflict":
          expect(o.reason).toBe("x");
          break;
        default: {
          const _exhaustive: never = o;
          throw new Error(`unreachable: ${JSON.stringify(_exhaustive)}`);
        }
      }
    }
  });
});

describe("ConflictClass values", () => {
  it("lists all three categories", () => {
    const classes: ConflictClass[] = [
      "NoPolicy",
      "PolicyConflict",
      "InvariantViolation",
    ];
    expect(classes).toHaveLength(3);
  });
});
