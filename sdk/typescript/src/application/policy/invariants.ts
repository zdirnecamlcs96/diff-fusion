/**
 * Tier 2 — post-merge invariants.
 *
 * Rules about *the result*, not about merging. A candidate merged value
 * must satisfy every registered invariant; if it doesn't, the outcome is
 * either a rewrite (`Transform`) or a rejection (`Reject`).
 *
 * Invariants run after Tier-1 policies produce a candidate. They never
 * choose between A and B — that's Tier 1's job. They only check that the
 * result is a valid entity.
 */

import type { JsonValue } from "../../domain/types.js";

export type InvariantOutcome =
  | { kind: "Pass" }
  | { kind: "Transform"; value: JsonValue }
  | { kind: "Reject"; reason: string };

/**
 * Pure predicate over (previous canonical, candidate). Implementors are
 * stateless and may return a replacement value via `Transform`.
 */
export interface Invariant {
  name(): string;
  check(previous: JsonValue, candidate: JsonValue): InvariantOutcome;
}

/**
 * A bundle of invariants applied in declared order. Stops at the first
 * `Reject`. A `Transform` rewrites the running candidate and chain
 * continues with the new value. Returns `Pass` when every invariant
 * accepted the value unchanged; `Transform` when any invariant in the
 * chain rewrote it.
 */
export class InvariantSet {
  private readonly invariants: Invariant[] = [];

  with(invariant: Invariant): this {
    this.invariants.push(invariant);
    return this;
  }

  apply(previous: JsonValue, candidate: JsonValue): InvariantOutcome {
    let current: JsonValue = candidate;
    for (const inv of this.invariants) {
      const outcome = inv.check(previous, current);
      switch (outcome.kind) {
        case "Pass":
          break;
        case "Transform":
          current = outcome.value;
          break;
        case "Reject":
          return {
            kind: "Reject",
            reason: `${inv.name()}: ${outcome.reason}`,
          };
        default: {
          const _exhaustive: never = outcome;
          throw new Error(
            `unreachable InvariantOutcome: ${JSON.stringify(_exhaustive)}`,
          );
        }
      }
    }
    return jsonEqual(current, candidate)
      ? { kind: "Pass" }
      : { kind: "Transform", value: current };
  }
}

function jsonEqual(a: JsonValue, b: JsonValue): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  if (typeof a !== typeof b) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!jsonEqual(a[i]!, b[i]!)) return false;
    }
    return true;
  }
  if (typeof a === "object" && typeof b === "object") {
    const aa = a as { [k: string]: JsonValue };
    const bb = b as { [k: string]: JsonValue };
    const aKeys = Object.keys(aa);
    const bKeys = Object.keys(bb);
    if (aKeys.length !== bKeys.length) return false;
    for (const k of aKeys) {
      if (!Object.prototype.hasOwnProperty.call(bb, k)) return false;
      if (!jsonEqual(aa[k]!, bb[k]!)) return false;
    }
    return true;
  }
  return false;
}
