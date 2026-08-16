/**
 * `SetByKey` — Tier 3 structural merges for collections of keyed objects.
 *
 * Per-field scalar policies (OwnedBy, Additive, etc.) cannot answer
 * "this line item exists in A but not B — was it added in A or deleted in
 * B?" That's a question about set identity, not a value-level one. This
 * policy answers it by declaring the composite business identity **and**
 * the stable per-side anchor fields that carry each system's local row ID.
 *
 * # Why anchors are required
 *
 * Every real integration involves one system that hands out immutable
 * local IDs (e.g. NetSuite `internalId`) and lets users rename business
 * fields (SKU, UOM, line number). Without a stable anchor, a rename
 * shows up as "element removed and a new one added", which corrupts the
 * three-way diff. Anchor **field names** are therefore mandatory config;
 * anchor **values** on individual elements may be absent (e.g. a row A
 * just created that hasn't roundtripped to B yet), in which case matching
 * falls through to the composite identity.
 *
 * # Nested line items
 *
 * Matched elements whose `onBothChanged === "Union"` get a shallow field
 * union by default. For any field named in `nested`, a sub-`SetByKey`
 * recursively merges that field as its own keyed array, preserving
 * per-line detail (e.g. `deliveryFulfillment[*].items[]` inside a matched
 * fulfillment).
 */

import { kernelMergeField } from "../../kernel.js";
import type { JsonValue } from "../../domain/types.js";
import type { SetByKeyRef } from "../../generated/policyConfig.js";
import type { MergePolicyRef } from "./declaration.js";
import type {
  FieldChange,
  MergeContext,
  MergeOutcome,
  MergePolicy,
} from "./index.js";

/** Whether side-only additions flow through to the merged array. */
export type OnAdded = "Include" | "Exclude";

/**
 * Removal-in-one-side disposition. `EscalateIfChanged` surfaces a
 * remove-vs-change collision as a conflict instead of silently preferring
 * one side.
 */
export type OnRemoved = "Remove" | "EscalateIfChanged";

/**
 * What to do when A and B both changed the same matched element (or both
 * added the same composite identity divergently).
 */
export type OnBothChanged = "Escalate" | "PreferA" | "PreferB" | "Union";

export class SetByKey implements MergePolicy {
  /**
   * Ordered list of business fields forming the cross-system identity
   * (e.g. `["sku", "uom"]`). Used when anchor rehoming doesn't match.
   */
  identity: readonly string[];
  /**
   * Stable A-side row identifier (e.g. internal `_id` / UUID). A-side
   * rows are rehomed to their ancestor row via this anchor *before*
   * composite-identity matching, so if A renames an identity field the
   * row still matches its former self.
   */
  aAnchor: string;
  /** Same as `aAnchor` but for B-side rows. */
  bAnchor: string;
  onAddedInA: OnAdded = "Include";
  onAddedInB: OnAdded = "Include";
  onRemovedInA: OnRemoved = "EscalateIfChanged";
  onRemovedInB: OnRemoved = "EscalateIfChanged";
  onBothChanged: OnBothChanged = "Escalate";
  /**
   * Only used when `onBothChanged === "Union"`. When true, A-side
   * values win on a per-field clash between the two matched elements;
   * when false, B wins.
   */
  preferAOnFieldConflict = true;
  /**
   * Per-field nested policies. When a matched pair is unioned, any field
   * name present here is recursively merged as its own keyed array rather
   * than shallow-overlaid.
   */
  nested = new Map<string, SetByKey>();

  constructor(identity: readonly string[], aAnchor: string, bAnchor: string) {
    this.identity = identity;
    this.aAnchor = aAnchor;
    this.bAnchor = bAnchor;
  }

  name(): string {
    return "set_by_key";
  }

  /**
   * Full wire declaration — the widened `MergePolicyRef::SetByKey` variant
   * carries every knob (`onAddedIn*` / `onRemovedIn*` / `onBothChanged`
   * / `preferAOnFieldConflict` / `nested`), so this round-trips the whole
   * instance. `merge()` delegates to the kernel through this ref.
   */
  toRef(): MergePolicyRef {
    return {
      kind: "set_by_key",
      identity: [...this.identity],
      a_anchor: this.aAnchor,
      b_anchor: this.bAnchor,
      on_added_in_a: this.onAddedInA,
      on_added_in_b: this.onAddedInB,
      on_removed_in_a: this.onRemovedInA,
      on_removed_in_b: this.onRemovedInB,
      on_both_changed: this.onBothChanged,
      prefer_a_on_field_conflict: this.preferAOnFieldConflict,
      nested: toNestedRef(this.nested),
    };
  }

  /**
   * Validate anchor / identity declarations against the CIF *field* schema
   * (the value at `schema.cif_schema.<path>`). Returns a list of human-
   * readable problems — empty means everything lines up.
   */
  validateAgainstSchema(fieldSchema: JsonValue): string[] {
    // Null / missing field schema — can't verify anchors. Surface so the
    // user either declares it or accepts the risk.
    if (fieldSchema === null) {
      return [
        "no CIF schema declared for this field; cannot verify anchor wiring",
      ];
    }
    if (!isPlainObject(fieldSchema)) {
      return ["field schema must be an object"];
    }
    const ty = typeof fieldSchema.type === "string" ? fieldSchema.type : undefined;
    if (ty !== "array") {
      return [
        `field declared as type=${JSON.stringify(ty)}, but set_by_key requires type='array'`,
      ];
    }
    const element = fieldSchema.element;
    if (element === undefined) {
      return [
        "array field has no 'element' schema; declare one to verify anchor wiring",
      ];
    }
    return this.validateAgainstElementSchema(element);
  }

  /**
   * Validate anchors / identity / nested declarations against a CIF
   * *element* schema — i.e. the `element` object inside an array field's
   * schema. Exposed so nested recursion can keep descending without
   * wrapping each level in a `{ type: "array", element }` indirection.
   */
  validateAgainstElementSchema(elementSchema: JsonValue): string[] {
    const errors: string[] = [];
    if (!isPlainObject(elementSchema)) {
      errors.push("element schema must be an object");
      return errors;
    }

    const checkAnchor = (label: string, field: string, expected: "a" | "b"): void => {
      const f = elementSchema[field];
      const role =
        isPlainObject(f) && typeof f.anchor === "string" ? f.anchor : undefined;
      if (role === expected) return;
      if (role === undefined) {
        errors.push(
          `${label} '${field}' is not declared as an anchor field in the element schema`,
        );
      } else {
        errors.push(
          `${label} '${field}' is declared with anchor='${role}', expected '${expected}'`,
        );
      }
    };
    checkAnchor("a_anchor", this.aAnchor, "a");
    checkAnchor("b_anchor", this.bAnchor, "b");

    for (const idField of this.identity) {
      if (!(idField in elementSchema)) {
        errors.push(
          `identity field '${idField}' not declared in element schema`,
        );
      }
    }

    for (const [nestedField, nestedPolicy] of this.nested) {
      const f = elementSchema[nestedField];
      if (f === undefined) {
        errors.push(
          `nested field '${nestedField}' not declared in element schema`,
        );
        continue;
      }
      if (!isPlainObject(f)) {
        errors.push(
          `nested field '${nestedField}' must be declared as type='array' in the element schema (got non-object)`,
        );
        continue;
      }
      const ty = typeof f.type === "string" ? f.type : undefined;
      if (ty !== "array") {
        errors.push(
          `nested field '${nestedField}' must be declared as type='array' in the element schema (got ${JSON.stringify(ty)})`,
        );
        continue;
      }
      const inner = f.element;
      if (inner === undefined) {
        errors.push(
          `nested field '${nestedField}' has no 'element' schema declared`,
        );
        continue;
      }
      for (const e of nestedPolicy.validateAgainstElementSchema(inner)) {
        errors.push(`nested.${nestedField}: ${e}`);
      }
    }

    return errors;
  }

  /**
   * Delegates to the kernel via the widened `set_by_key` wire shape (see
   * `toRef()`), which now carries every knob this class exposes.
   */
  merge(change: FieldChange, ctx: MergeContext): MergeOutcome {
    return kernelMergeField(change, this.toRef(), ctx);
  }
}

// ---------- helpers ----------

/** Recursively convert a `nested` map to its wire shape for `toRef()`. */
function toNestedRef(nested: ReadonlyMap<string, SetByKey>): Record<string, SetByKeyRef> {
  const out: Record<string, SetByKeyRef> = {};
  for (const [field, policy] of nested) {
    out[field] = {
      identity: [...policy.identity],
      a_anchor: policy.aAnchor,
      b_anchor: policy.bAnchor,
      on_added_in_a: policy.onAddedInA,
      on_added_in_b: policy.onAddedInB,
      on_removed_in_a: policy.onRemovedInA,
      on_removed_in_b: policy.onRemovedInB,
      on_both_changed: policy.onBothChanged,
      prefer_a_on_field_conflict: policy.preferAOnFieldConflict,
      nested: toNestedRef(policy.nested),
    };
  }
  return out;
}

function isPlainObject(
  value: JsonValue | undefined,
): value is { [k: string]: JsonValue } {
  return (
    value !== null &&
    value !== undefined &&
    typeof value === "object" &&
    !Array.isArray(value)
  );
}
