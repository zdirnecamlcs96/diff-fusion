/**
 * Dotted-path writer over `JsonValue`.
 *
 * Three-way diff emits changelog paths like `"pricing.amount"` (dotted
 * object keys). This module provides the inverse operation: given a path
 * and a value, set the leaf at that path, creating any missing
 * intermediate objects. Non-object values along the path are replaced
 * with a fresh object so descent can continue — matching the Rust
 * `set_at_path` behaviour.
 *
 * Paths do not support array indexing; the diff primitive emits array
 * mismatches as a single change at the array's own path.
 */

import type { JsonValue } from "./types.js";

export function setAtPath(target: JsonValue, path: string, newValue: JsonValue): void {
  if (path === "") return;
  if (!isMutableObject(target)) return;
  const parts = path.split(".");
  setRecursive(target, parts, newValue);
}

function setRecursive(
  target: { [k: string]: JsonValue },
  parts: string[],
  newValue: JsonValue,
): void {
  const [head, ...rest] = parts;
  if (head === undefined) return;

  if (rest.length === 0) {
    target[head] = newValue;
    return;
  }

  const existing = target[head];
  let child: { [k: string]: JsonValue };
  if (isMutableObject(existing)) {
    child = existing;
  } else {
    child = {};
    target[head] = child;
  }
  setRecursive(child, rest, newValue);
}

function isMutableObject(v: JsonValue | undefined): v is { [k: string]: JsonValue } {
  return v !== undefined && v !== null && typeof v === "object" && !Array.isArray(v);
}
