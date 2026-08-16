import { describe, expect, it } from "vitest";
import {
  MergeContext,
  type MergeOutcome,
} from "../../../../src/application/policy/index.js";
import {
  OnBothChanged,
  SetByKey,
} from "../../../../src/application/policy/structural.js";
import { threeWayDiff } from "../../../../src/domain/diff/threeWay.js";
import type { JsonValue } from "../../../../src/domain/types.js";

function ctx(): MergeContext {
  return new MergeContext("a", "b");
}

/**
 * Mirror of the Rust `policy_by` helper: a one-field-identity `SetByKey`
 * where the anchor happens to be the same field as the identity, so anchor
 * rehoming collapses to a no-op on simple fixtures.
 */
function policyBy(id: string): SetByKey {
  return new SetByKey([id], id, id);
}

function resolvedArray(outcome: MergeOutcome): JsonValue[] {
  if (outcome.kind !== "Resolved") {
    throw new Error(`expected Resolved, got ${JSON.stringify(outcome)}`);
  }
  if (!Array.isArray(outcome.value)) {
    throw new Error(`expected array, got ${JSON.stringify(outcome.value)}`);
  }
  return outcome.value;
}

describe("SetByKey.name", () => {
  it("returns 'set_by_key' (matches Rust)", () => {
    expect(policyBy("sku").name()).toBe("set_by_key");
  });
});

describe("additions_on_both_sides_merge", () => {
  it("accepts additions from both sides", () => {
    const anc: JsonValue = { items: [{ sku: "x", q: 1 }] };
    const a: JsonValue = { items: [{ sku: "x", q: 1 }, { sku: "y", q: 2 }] };
    const b: JsonValue = { items: [{ sku: "x", q: 1 }, { sku: "z", q: 3 }] };
    const log = threeWayDiff(anc, a, b);
    const outcome = policyBy("sku").merge(log.changes[0]!, ctx());
    const arr = resolvedArray(outcome);
    expect(arr).toHaveLength(3);
    const skus = arr.map((e) => (e as { sku: string }).sku);
    expect(skus).toContain("x");
    expect(skus).toContain("y");
    expect(skus).toContain("z");
  });
});

describe("removal_in_a_unchanged_in_b_drops_element", () => {
  it("drops an A-removed element when B did not touch it", () => {
    const anc: JsonValue = { items: [{ sku: "x" }, { sku: "y" }] };
    const a: JsonValue = { items: [{ sku: "x" }] };
    const b: JsonValue = { items: [{ sku: "x" }, { sku: "y" }] };
    const log = threeWayDiff(anc, a, b);
    const outcome = policyBy("sku").merge(log.changes[0]!, ctx());
    const arr = resolvedArray(outcome);
    expect(arr).toHaveLength(1);
    expect((arr[0] as { sku: string }).sku).toBe("x");
  });
});

describe("removal_in_a_but_changed_in_b_escalates", () => {
  it("surfaces remove-vs-change as a conflict", () => {
    const anc: JsonValue = { items: [{ sku: "x", q: 1 }] };
    const a: JsonValue = { items: [] };
    const b: JsonValue = { items: [{ sku: "x", q: 99 }] };
    const log = threeWayDiff(anc, a, b);
    const outcome = policyBy("sku").merge(log.changes[0]!, ctx());
    expect(outcome.kind).toBe("Conflict");
    if (outcome.kind === "Conflict") {
      expect(outcome.reason).toContain("'x'");
      expect(outcome.reason).toContain("removed in A");
    }
  });
});

describe("both_changed_escalates_by_default", () => {
  it("returns Conflict when on_both_changed is Escalate (default)", () => {
    const anc: JsonValue = { items: [{ sku: "x", q: 1 }] };
    const a: JsonValue = { items: [{ sku: "x", q: 10 }] };
    const b: JsonValue = { items: [{ sku: "x", q: 20 }] };
    const log = threeWayDiff(anc, a, b);
    const outcome = policyBy("sku").merge(log.changes[0]!, ctx());
    expect(outcome.kind).toBe("Conflict");
    if (outcome.kind === "Conflict") {
      expect(outcome.reason).toContain("changed on both");
    }
  });
});

describe("missing_identity_is_a_conflict", () => {
  it("returns Conflict when an element lacks its identity field", () => {
    const anc: JsonValue = { items: [{ q: 1 }] };
    const a: JsonValue = { items: [{ q: 2 }] };
    const b: JsonValue = { items: [{ q: 1 }] };
    const log = threeWayDiff(anc, a, b);
    const outcome = policyBy("sku").merge(log.changes[0]!, ctx());
    expect(outcome.kind).toBe("Conflict");
  });
});

describe("anchor_rehomes_row_across_identity_mutation", () => {
  it("treats an A-side identity rename as a field edit via anchor", () => {
    // A renamed the identity field from "old" to "new" for the row whose
    // stable anchor is id=1. Without anchor rehoming this would look like
    // "old" removed + "new" added; with anchor it's a plain field edit.
    const anc: JsonValue = { items: [{ sku: "old", id: 1, q: 5 }] };
    const a: JsonValue = { items: [{ sku: "new", id: 1, q: 5 }] };
    const b: JsonValue = { items: [{ sku: "old", id: 1, q: 5 }] };
    const log = threeWayDiff(anc, a, b);
    const policy = new SetByKey(["sku"], "id", "id");
    const outcome = policy.merge(log.changes[0]!, ctx());
    const arr = resolvedArray(outcome);
    expect(arr).toHaveLength(1);
    const row = arr[0] as { sku: string; id: number };
    expect(row.sku).toBe("new");
    expect(row.id).toBe(1);
  });
});

describe("union_strategy_merges_matched_element_fields", () => {
  it("preserves extra fields from both sides under Union", () => {
    const anc: JsonValue = { items: [{ sku: "x", q: 1 }] };
    const a: JsonValue = { items: [{ sku: "x", q: 1, a_extra: true }] };
    const b: JsonValue = { items: [{ sku: "x", q: 1, b_extra: 42 }] };
    const log = threeWayDiff(anc, a, b);
    const policy = policyBy("sku");
    policy.onBothChanged = "Union" satisfies OnBothChanged;
    const outcome = policy.merge(log.changes[0]!, ctx());
    const arr = resolvedArray(outcome);
    expect(arr).toHaveLength(1);
    const row = arr[0] as { a_extra?: boolean; b_extra?: number };
    expect(row.a_extra).toBe(true);
    expect(row.b_extra).toBe(42);
  });
});

describe("validate_against_element_schema_passes_when_anchors_declared", () => {
  it("returns no errors when anchors + identity are properly declared", () => {
    const schema: JsonValue = {
      externalId: { type: "string", anchor: "a" },
      internalId: { type: "string", anchor: "b" },
      sku: { type: "string" },
      uom: { type: "string" },
    };
    const policy = new SetByKey(["sku", "uom"], "externalId", "internalId");
    expect(policy.validateAgainstElementSchema(schema)).toEqual([]);
  });
});

describe("validate_against_element_schema_flags_missing_anchor", () => {
  it("flags a missing anchor declaration on the element schema", () => {
    const schema: JsonValue = {
      externalId: { type: "string", anchor: "a" },
      sku: { type: "string" },
    };
    const policy = new SetByKey(["sku"], "externalId", "internalId");
    const errs = policy.validateAgainstElementSchema(schema);
    expect(errs).toHaveLength(1);
    expect(errs[0]).toContain("b_anchor 'internalId'");
  });
});

describe("validate_against_element_schema_flags_wrong_anchor_role", () => {
  it("flags anchors declared with the wrong role", () => {
    const schema: JsonValue = {
      externalId: { type: "string", anchor: "b" }, // role flipped
      internalId: { type: "string", anchor: "a" }, // role flipped
      sku: { type: "string" },
    };
    const policy = new SetByKey(["sku"], "externalId", "internalId");
    const errs = policy.validateAgainstElementSchema(schema);
    expect(errs).toHaveLength(2);
    expect(errs.some((e) => e.includes("expected 'a'"))).toBe(true);
    expect(errs.some((e) => e.includes("expected 'b'"))).toBe(true);
  });
});

describe("validate_against_element_schema_recurses_into_nested", () => {
  it("recursively validates nested SetByKey policies against their element schemas", () => {
    const schema: JsonValue = {
      externalId: { type: "string", anchor: "a" },
      internalId: { type: "string", anchor: "b" },
      sku: { type: "string" },
      subLines: {
        type: "array",
        element: {
          extSubId: { type: "string", anchor: "a" },
          intSubId: { type: "string", anchor: "b" },
          sku: { type: "string" },
        },
      },
    };
    const policy = new SetByKey(["sku"], "externalId", "internalId");
    policy.nested.set(
      "subLines",
      new SetByKey(["sku"], "extSubId", "intSubId"),
    );
    expect(policy.validateAgainstElementSchema(schema)).toEqual([]);
  });
});

describe("nested_policy_merges_sub_array_recursively", () => {
  it("reconciles nested arrays when both sides add distinct children", () => {
    const anc: JsonValue = {
      groups: [{ gid: "G1", items: [{ sku: "A", q: 1 }] }],
    };
    const a: JsonValue = {
      groups: [
        { gid: "G1", items: [{ sku: "A", q: 1 }, { sku: "B", q: 2 }] },
      ],
    };
    const b: JsonValue = {
      groups: [
        { gid: "G1", items: [{ sku: "A", q: 1 }, { sku: "C", q: 3 }] },
      ],
    };
    const log = threeWayDiff(anc, a, b);

    const outer = policyBy("gid");
    outer.onBothChanged = "Union" satisfies OnBothChanged;
    outer.nested.set("items", policyBy("sku"));

    const outcome = outer.merge(log.changes[0]!, ctx());
    const arr = resolvedArray(outcome);
    expect(arr).toHaveLength(1);
    const group = arr[0] as { items: Array<{ sku: string }> };
    expect(group.items).toHaveLength(3);
    const skus = group.items.map((i) => i.sku);
    expect(skus).toContain("A");
    expect(skus).toContain("B");
    expect(skus).toContain("C");
  });
});

describe("SetByKey.validateAgainstSchema (MergePolicy hook)", () => {
  it("reports missing field schema when fieldSchema is null", () => {
    const policy = new SetByKey(["sku"], "externalId", "internalId");
    const errs = policy.validateAgainstSchema(null);
    expect(errs).toHaveLength(1);
    expect(errs[0]).toContain("no CIF schema");
  });

  it("reports wrong type when the field is not declared as array", () => {
    const policy = new SetByKey(["sku"], "externalId", "internalId");
    const errs = policy.validateAgainstSchema({ type: "object" });
    expect(errs).toHaveLength(1);
    expect(errs[0]).toContain("requires type='array'");
  });

  it("reports missing element schema on an array field", () => {
    const policy = new SetByKey(["sku"], "externalId", "internalId");
    const errs = policy.validateAgainstSchema({ type: "array" });
    expect(errs).toHaveLength(1);
    expect(errs[0]).toContain("no 'element' schema");
  });

  it("recurses into element schema and passes when anchors are declared", () => {
    const policy = new SetByKey(["sku"], "externalId", "internalId");
    const fieldSchema: JsonValue = {
      type: "array",
      element: {
        externalId: { type: "string", anchor: "a" },
        internalId: { type: "string", anchor: "b" },
        sku: { type: "string" },
      },
    };
    expect(policy.validateAgainstSchema(fieldSchema)).toEqual([]);
  });
});
