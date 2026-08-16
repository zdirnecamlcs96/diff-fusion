/**
 * Recursive structural comparison for JSON values.
 *
 * Returns an array of `[path, [oldValue, newValue]]` pairs for every leaf
 * that differs between `a` and `b`. Objects are walked recursively; arrays
 * and scalars are compared by value — a mismatch at either emits a single
 * change at the current path (no per-element array diffing).
 *
 * Delegates to the Rust kernel (SSOT) via `kernelCompareJson`; differential
 * fuzz-verified equivalent to the retired native body (kernel-v2 Phase 1).
 */

import type { JsonValue } from "./types.js";
import { kernelCompareJson } from "../kernel.js";

export type ComparePath = string;
export type CompareChange = readonly [ComparePath, readonly [JsonValue, JsonValue]];

export function compareJson(a: JsonValue, b: JsonValue): CompareChange[] {
  return kernelCompareJson(a, b);
}
