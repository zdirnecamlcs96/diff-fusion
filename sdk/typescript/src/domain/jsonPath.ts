/**
 * Dotted-path writer over `JsonValue`.
 *
 * Three-way diff emits changelog paths like `"pricing.amount"` (dotted
 * object keys). Object keys that themselves contain a literal `.` or `\`
 * are escaped per `escapeSegment` before joining, so a key like `"a.b"`
 * becomes the path segment `"a\.b"` — indistinguishable from a *real* path
 * separator only if you don't run it back through `splitPath`, which
 * undoes the escaping. This module provides the inverse operation: given
 * a path and a value, set the leaf at that path, creating any missing
 * intermediate objects. Non-object values along the path are replaced
 * with a fresh object so descent can continue — matching the Rust
 * `set_at_path` behaviour.
 *
 * Paths do not support array indexing; the diff primitive emits array
 * mismatches as a single change at the array's own path.
 */

import type { JsonValue } from "./types.js";

/**
 * Escape a single path segment so a literal `.` or `\` inside it survives
 * `splitPath` as part of the segment instead of being read as a separator
 * or escape character. Order matters: backslashes are escaped first, then
 * dots — otherwise a dot-escape's own backslash would itself get
 * re-escaped.
 */
export function escapeSegment(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/\./g, "\\.");
}

/**
 * Split a path produced by joining `escapeSegment`-escaped segments with
 * `.` back into its original segments. Scans left to right: `\` consumes
 * the next character literally (whatever it is) into the current segment,
 * an unescaped `.` ends the current segment, and a trailing lone `\` is
 * kept as a literal backslash. Never throws — every input has a
 * well-defined split.
 */
export function splitPath(path: string): string[] {
  const parts: string[] = [];
  let current = "";
  for (let i = 0; i < path.length; i++) {
    const c = path[i];
    if (c === "\\") {
      i++;
      current += i < path.length ? path[i] : "\\";
    } else if (c === ".") {
      parts.push(current);
      current = "";
    } else {
      current += c;
    }
  }
  parts.push(current);
  return parts;
}

export function setAtPath(target: JsonValue, path: string, newValue: JsonValue): void {
  if (path === "") return;
  if (!isMutableObject(target)) return;
  const parts = splitPath(path);
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
