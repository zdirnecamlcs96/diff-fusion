/**
 * Per-field merge policies — Tier 1 of the policy stack.
 *
 * Each `MergePolicy` decides how to reconcile one `FieldChange` from the
 * three-way diff. Policies are declarative and pure: given a change and a
 * context, they return a `MergeOutcome`. They never throw, never write,
 * never observe the clock.
 *
 * `resolve()` dispatches each `FieldChange` to the policy declared for its
 * path. Unresolved paths (no policy, or policy returns `Conflict`) accumulate
 * in `Resolution.conflicts` for the escalation queue.
 */

import type { JsonValue } from "../../domain/types.js";
import type {
  Changelog,
  FieldChange,
} from "../../domain/diff/threeWay.js";
import { mergeOutcomeSchema, type MergeOutcome } from "../../generated/mergeOutcome.js";

// Re-export the canonical Changelog shape from the domain layer so individual
// policy modules have a single import surface.
export type {
  ChangeSource,
  Changelog,
  FieldChange,
} from "../../domain/diff/threeWay.js";

/**
 * Labels for the two sides of a three-way diff, so per-field policies can
 * tell "A" and "B" apart by name (e.g. "netsuite" vs "our_inventory").
 */
export class MergeContext {
  readonly system_a: string;
  readonly system_b: string;

  constructor(systemA: string, systemB: string) {
    this.system_a = systemA;
    this.system_b = systemB;
  }
}

/** The outcome of applying one policy to one `FieldChange`. */
export { mergeOutcomeSchema, type MergeOutcome };

/**
 * Pure per-field merge rule. Implementors are stateless and cheap to
 * construct (usually empty classes or small config).
 */
export interface MergePolicy {
  /** Stable name for logging, error messages, and the policy registry. */
  name(): string;

  /** Apply this policy to a single change. */
  merge(change: FieldChange, ctx: MergeContext): MergeOutcome;

  /**
   * Validate this policy's config against the CIF schema definition for the
   * field it will govern. `fieldSchema` is the value at
   * `schema.cif_schema.<path>` — policies that care about element structure
   * (e.g. `SetByKey`) read `.element` from it and verify their anchor /
   * identity fields are declared there. Default: no errors.
   */
  validateAgainstSchema?(fieldSchema: JsonValue): string[];
}

/**
 * Cause category for an unresolved conflict. Enables per-class disposition
 * (reject / escalate / preserve-both) at the user layer, per the
 * Dropbox/Synology conflict-visibility pattern.
 *
 * Values kept as the Rust variant names (PascalCase) so JSON logs are
 * interchangeable.
 */
export type ConflictClass =
  | "NoPolicy"
  | "PolicyConflict"
  | "InvariantViolation";

/** A conflict that survived resolution and must go to the escalation queue. */
export interface UnresolvedConflict {
  path: string;
  reason: string;
  class: ConflictClass;
  change: FieldChange;
}

/**
 * Per-path resolution result. `resolved` lists fields the orchestrator should
 * write; `conflicts` lists fields to escalate. Deterministic iteration order
 * because `Map` preserves insertion order.
 */
export class Resolution {
  readonly resolved: Map<string, JsonValue>;
  readonly conflicts: UnresolvedConflict[];

  constructor(
    resolved: Map<string, JsonValue> = new Map(),
    conflicts: UnresolvedConflict[] = [],
  ) {
    this.resolved = resolved;
    this.conflicts = conflicts;
  }

  isClean(): boolean {
    return this.conflicts.length === 0;
  }
}

/**
 * Map from canonical field path to the policy that governs it.
 *
 * An optional default policy catches paths not explicitly declared — if not
 * set, unregistered paths become `Conflict` with class `NoPolicy` and
 * escalate.
 */
export class PolicyMap {
  private readonly byPath = new Map<string, MergePolicy>();
  private default_?: MergePolicy;

  with(path: string, policy: MergePolicy): this {
    this.byPath.set(path, policy);
    return this;
  }

  withDefault(policy: MergePolicy): this {
    this.default_ = policy;
    return this;
  }

  /**
   * Look up the policy for a path. Exact match first, then the default.
   * Returns `undefined` when neither is registered.
   */
  lookup(path: string): MergePolicy | undefined {
    return this.byPath.get(path) ?? this.default_;
  }

  /**
   * Validate every registered policy against a full CIF schema JSON (the
   * structure with a top-level `cif_schema` object). Returns an aggregated
   * list of problems with each entry prefixed by its path. An empty array
   * means everything lines up.
   */
  validateAgainstSchema(schema: JsonValue): string[] {
    const errors: string[] = [];
    const cifSchema = isPlainObject(schema)
      ? (schema["cif_schema"] ?? null)
      : null;
    for (const [path, policy] of this.byPath) {
      const fieldSchema: JsonValue =
        isPlainObject(cifSchema) && path in cifSchema
          ? (cifSchema[path] as JsonValue)
          : null;
      if (policy.validateAgainstSchema) {
        for (const e of policy.validateAgainstSchema(fieldSchema)) {
          errors.push(`${path}: ${e}`);
        }
      }
    }
    return errors;
  }
}

/**
 * Apply a `PolicyMap` to every change in a `Changelog`.
 *
 * A change with `ChangeSource::A` or `B` alone is always resolvable by
 * trivial rule — one side moved, the other did not, so the mover wins unless
 * an owner-based policy says otherwise. This function defers the decision to
 * the policy in all cases so overrides like `OwnedBy` (which can veto a
 * non-owner's change) work uniformly.
 */
export function resolve(
  changelog: Changelog,
  policies: PolicyMap,
  ctx: MergeContext,
): Resolution {
  const resolved = new Map<string, JsonValue>();
  const conflicts: UnresolvedConflict[] = [];

  for (const change of changelog.changes) {
    const policy = policies.lookup(change.path);
    if (policy === undefined) {
      conflicts.push({
        path: change.path,
        reason: `no policy declared for path '${change.path}'`,
        class: "NoPolicy",
        change,
      });
      continue;
    }

    const outcome = policy.merge(change, ctx);
    switch (outcome.kind) {
      case "Resolved":
        resolved.set(change.path, outcome.value);
        break;
      case "Conflict":
        conflicts.push({
          path: change.path,
          reason: `${policy.name()}: ${outcome.reason}`,
          class: "PolicyConflict",
          change,
        });
        break;
      default: {
        const _exhaustive: never = outcome;
        throw new Error(
          `unreachable MergeOutcome: ${JSON.stringify(_exhaustive)}`,
        );
      }
    }
  }

  return new Resolution(resolved, conflicts);
}

function isPlainObject(
  value: JsonValue | null | undefined,
): value is { [k: string]: JsonValue } {
  return (
    value !== null &&
    value !== undefined &&
    typeof value === "object" &&
    !Array.isArray(value)
  );
}
