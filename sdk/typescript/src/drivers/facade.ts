/**
 * Tier-0 `DiffFusion` facade — detection only.
 *
 * `DiffFusion` composes the schema-driven transformer (`application/transform`)
 * with the two-way comparator (`domain/compare`). It has no reconciliation
 * opinions: it transforms source data to CIF, compares two CIF values, and
 * reports the differences. Tier-1 `SyncEngine` (separate driver) wraps the
 * same primitives with policies, invariants, ancestor storage, and an
 * escalation queue.
 *
 * The API mirrors Rust's `DiffFusion` 1:1 — `new`, `transform`, `compare`,
 * `transformAndCompare`, `validateCif`, `schema()`.
 */

import type { JsonValue } from "../domain/types.js";
import { compareJson } from "../domain/compare.js";
import { toCif, type TransformResult } from "../application/transform.js";

/**
 * One detected difference between two CIF values.
 *
 * `oldValue` / `newValue` are stringified representations of the leaf values
 * at `path`. The Rust facade uses Rust `Debug` format here; TS uses
 * `JSON.stringify` because there's no cross-runtime fixture asserting the
 * string form and JSON output is the obvious stringification in JS.
 */
export interface Conflict {
  path: string;
  oldValue: string;
  newValue: string;
}

/** Summary of conflict detection between two CIF values. */
export interface ConflictReport {
  conflicts: Conflict[];
  hasConflicts: boolean;
  totalConflicts: number;
}

/** Discriminated result of any compare operation that may have failed upstream. */
export type CompareResult =
  | { ok: true; report: ConflictReport }
  | { ok: false; error: string };

/**
 * Tier-0 facade: transform + detect. No reconciliation, no I/O, no state
 * beyond the schema. Safe to share across cycles; methods are pure.
 */
export class DiffFusion {
  private readonly schema_: JsonValue;

  constructor(schema: JsonValue) {
    this.schema_ = schema;
  }

  /** Read-only access to the schema the facade is bound to. */
  schema(): JsonValue {
    return this.schema_;
  }

  /**
   * Transform source data to CIF using `schema.transformations[formatId]`.
   *
   * Returns a discriminated result — `{ ok: true, value }` on success,
   * `{ ok: false, error }` when the schema is malformed or a required source
   * field is missing. Matches the in-domain return convention (plan §5).
   */
  transform(source: JsonValue, formatId: string): TransformResult {
    return toCif(source, this.schema_, formatId);
  }

  /**
   * Compare two CIF values and produce a flat conflict report. Paths are
   * dotted; nested objects walked recursively; arrays compared structurally
   * at the enclosing path (no per-element diffing at this layer — that's
   * `SetByKey`'s job in Tier-1).
   */
  compare(oldValue: JsonValue, newValue: JsonValue): ConflictReport {
    const diffs = compareJson(oldValue, newValue);
    const conflicts: Conflict[] = diffs.map(([path, [oldVal, newVal]]) => ({
      path,
      oldValue: stringify(oldVal),
      newValue: stringify(newVal),
    }));
    return {
      conflicts,
      hasConflicts: conflicts.length > 0,
      totalConflicts: conflicts.length,
    };
  }

  /**
   * End-to-end: transform both sources to CIF, then compare the results.
   *
   * Returns {@link CompareResult} so callers can distinguish transform
   * failures (malformed schema or missing required field) from a successful
   * report that happens to contain zero conflicts.
   */
  transformAndCompare(
    sourceA: JsonValue,
    formatA: string,
    sourceB: JsonValue,
    formatB: string,
  ): CompareResult {
    const cifA = this.transform(sourceA, formatA);
    if (!cifA.ok) return { ok: false, error: cifA.error };
    const cifB = this.transform(sourceB, formatB);
    if (!cifB.ok) return { ok: false, error: cifB.error };
    return { ok: true, report: this.compare(cifA.value, cifB.value) };
  }

  /**
   * Validate a CIF value against the schema's `cif_schema` definition.
   *
   * Checks: required fields are present, and declared leaf types match.
   * Returns `{ ok: true }` on success or `{ ok: false, errors }` with one
   * message per problem. Mirrors Rust's `validate_cif` semantics (same error
   * messages so logs are interchangeable).
   */
  validateCif(value: JsonValue): { ok: true } | { ok: false; errors: string[] } {
    const errors: string[] = [];

    const schemaObj = asObject(this.schema_);
    const cifSchema = schemaObj?.["cif_schema"];
    if (cifSchema === undefined) {
      return { ok: false, errors: ["Schema missing 'cif_schema' definition"] };
    }

    const valueObj = asObject(value);
    if (valueObj === undefined) {
      return { ok: false, errors: ["CIF value must be an object"] };
    }

    const cifFields = asObject(cifSchema);
    if (cifFields === undefined) {
      return { ok: false, errors: ["cif_schema must be an object"] };
    }

    for (const [fieldName, fieldDefRaw] of Object.entries(cifFields)) {
      const fieldDef = asObject(fieldDefRaw);
      const isRequired =
        fieldDef !== undefined && fieldDef["required"] === true;

      if (isRequired && !(fieldName in valueObj)) {
        errors.push(`Missing required field: ${fieldName}`);
      }

      const fieldValue = valueObj[fieldName];
      const expectedType =
        fieldDef !== undefined && typeof fieldDef["type"] === "string"
          ? fieldDef["type"]
          : undefined;

      if (fieldValue !== undefined && expectedType !== undefined) {
        const actualType = typeOfJson(fieldValue);
        if (actualType !== expectedType) {
          errors.push(
            `Field '${fieldName}': expected type '${expectedType}', got '${actualType}'`,
          );
        }
      }
    }

    if (errors.length === 0) return { ok: true };
    return { ok: false, errors };
  }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function stringify(v: JsonValue): string {
  return JSON.stringify(v);
}

function asObject(v: JsonValue | undefined): { [k: string]: JsonValue } | undefined {
  if (v === null || v === undefined) return undefined;
  if (typeof v !== "object") return undefined;
  if (Array.isArray(v)) return undefined;
  return v;
}

function typeOfJson(v: JsonValue): "string" | "number" | "boolean" | "object" | "array" | "null" {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  const t = typeof v;
  if (t === "string" || t === "number" || t === "boolean" || t === "object") {
    return t;
  }
  // Unreachable: JsonValue excludes undefined, symbol, function, bigint.
  throw new Error(`unexpected runtime type for JsonValue: ${t}`);
}
