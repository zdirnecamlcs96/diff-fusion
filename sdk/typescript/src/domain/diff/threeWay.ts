/**
 * Three-way diff against a stored ancestor.
 *
 * For every leaf touched by A or B relative to the ancestor, emits a
 * `FieldChange` recording *which side moved*. That provenance signal is
 * what lets the policy layer resolve conflicts — without it, reconciliation
 * degenerates into time-based tie-breaking.
 *
 * Semantics:
 * - A changed, B unchanged → source = "a"
 * - A unchanged, B changed → source = "b"
 * - Both moved (even to the same value) → source = "both"
 *   "both" is not a conflict; the resolver decides whether the values agree.
 *
 * Delegates to the Rust WASM kernel via `kernelThreeWayDiff` — no
 * re-implemented leaf comparison.
 */

import { kernelThreeWayDiff } from "../../kernel.js";
import type { JsonValue } from "../types.js";

export type ChangeSource = "a" | "b" | "both";

export interface FieldChange {
  path: string;
  oldValue: JsonValue;
  newFromA: JsonValue | undefined;
  newFromB: JsonValue | undefined;
  source: ChangeSource;
}

export interface Changelog {
  changes: FieldChange[];
}

export function changelogIsEmpty(log: Changelog): boolean {
  return log.changes.length === 0;
}

export function threeWayDiff(ancestor: JsonValue, a: JsonValue, b: JsonValue): Changelog {
  return kernelThreeWayDiff(ancestor, a, b);
}
