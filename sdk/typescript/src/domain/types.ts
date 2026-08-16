import { z } from "zod";

/**
 * Recursive JSON value type. Matches `serde_json::Value` semantics on the Rust
 * side: objects are string-keyed maps, arrays are ordered, numbers are finite
 * f64. Use `unknown`-narrowed callers, never `any`.
 */
export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [k: string]: JsonValue };

/**
 * Runtime mirror of {@link JsonValue}. The `z.ZodType<JsonValue>` annotation
 * pins the schema to the hand-written alias so an accidental widening here
 * can't silently produce a merely-assignable different type.
 */
export const jsonValueSchema: z.ZodType<JsonValue> = z.lazy(() =>
  z.union([
    z.null(),
    z.boolean(),
    z.number().finite(),
    z.string(),
    z.array(jsonValueSchema),
    z.record(z.string(), jsonValueSchema),
  ]),
);

/**
 * Marks a CIF element field as the stable anchor for one side of a
 * cross-system merge. `SetByKey` uses these to re-home a row whose identity
 * field mutated on one side — without them a rename looks like a remove+add
 * and corrupts three-way diffing.
 *
 * Wire format mirrors Rust `#[serde(rename_all = "lowercase")]`: `"a"` | `"b"`.
 */
export type AnchorRole = "a" | "b";

/**
 * Base types supported in a CIF schema. Wire format is the lowercase string
 * (matches Rust `#[serde(rename_all = "lowercase")]`), so the string-literal
 * union doubles as both the runtime tag and the JSON encoding.
 */
export type CifType =
  | "string"
  | "number"
  | "boolean"
  | "object"
  | "array"
  | "null";

export const CIF_TYPES: readonly CifType[] = [
  "string",
  "number",
  "boolean",
  "object",
  "array",
  "null",
];

/** Parse a `CifType` from a string. Returns `undefined` for unknown inputs. */
export function cifTypeFromString(s: string): CifType | undefined {
  const lower = s.toLowerCase();
  return (CIF_TYPES as readonly string[]).includes(lower)
    ? (lower as CifType)
    : undefined;
}

/** Identity on the wire — kept for parity with Rust `Display`. */
export function cifTypeToString(t: CifType): string {
  return t;
}

export function isNullable(t: CifType): boolean {
  return t === "null";
}

export function isPrimitive(t: CifType): boolean {
  return t === "string" || t === "number" || t === "boolean";
}

export function isCollection(t: CifType): boolean {
  return t === "array" || t === "object";
}

/**
 * Conflict resolution strategies for fields without an explicit source of
 * truth.
 *
 * **Deprecated.** Predates the tiered policy stack in
 * `application/policy`; kept for backward compatibility with existing schema
 * JSON. New code should declare behaviour via `MergePolicy` implementations.
 *
 * Wire values are Rust `#[serde(rename_all = "snake_case")]`.
 */
export type ConflictStrategy =
  | "last_write_wins"
  | "prefer_a"
  | "prefer_b"
  | "manual_resolve"
  | "use_max"
  | "use_min"
  | "merge";

/**
 * Field definition in a CIF schema. Optional fields are `undefined` (not
 * `null`) and are stripped from JSON output to match Rust's
 * `#[serde(skip_serializing_if = "Option::is_none")]`.
 */
export interface CifFieldDefinition {
  /** Wire name matches Rust `#[serde(rename = "type")]`. */
  type: string;
  required: boolean;
  description?: string;
  default?: JsonValue;
  source_of_truth?: string;
  conflict_strategy?: ConflictStrategy;
  /**
   * For `type === "array"`, declares the shape of each element. The
   * transformer walks source arrays element-by-element; `SetByKey` validates
   * that declared anchor fields exist.
   */
  element?: Record<string, CifFieldDefinition>;
  /**
   * Marks this field as a per-side stable anchor for the row it belongs to.
   * Only meaningful on element fields inside an array.
   */
  anchor?: AnchorRole;
}

/**
 * Fluent builder for `CifFieldDefinition`. Mirrors the Rust chainable API.
 * Call `build()` to obtain a plain-object `CifFieldDefinition` with
 * `undefined` optionals stripped (so JSON round-trips match Rust's
 * `skip_serializing_if = "Option::is_none"`).
 */
export class CifFieldDefinitionBuilder {
  private readonly def: CifFieldDefinition;

  constructor(fieldType: string) {
    this.def = { type: fieldType, required: false };
  }

  required(): this {
    this.def.required = true;
    return this;
  }

  optional(): this {
    this.def.required = false;
    return this;
  }

  withDescription(desc: string): this {
    this.def.description = desc;
    return this;
  }

  withDefault(value: JsonValue): this {
    this.def.default = value;
    return this;
  }

  withSourceOfTruth(system: string): this {
    this.def.source_of_truth = system;
    return this;
  }

  withConflictStrategy(strategy: ConflictStrategy): this {
    this.def.conflict_strategy = strategy;
    return this;
  }

  withElement(element: Record<string, CifFieldDefinition>): this {
    this.def.element = element;
    return this;
  }

  withAnchor(role: AnchorRole): this {
    this.def.anchor = role;
    return this;
  }

  build(): CifFieldDefinition {
    return stripUndefined({ ...this.def });
  }
}

/** Shorthand factory matching the Rust `CifFieldDefinition::new(...)` entry point. */
export function cifFieldDefinition(
  fieldType: string,
): CifFieldDefinitionBuilder {
  return new CifFieldDefinitionBuilder(fieldType);
}

export type ValidationResult =
  | { ok: true }
  | { ok: false; error: string };

export type SchemaValidationResult =
  | { ok: true }
  | { ok: false; errors: string[] };

/** Structural validation for a single field definition. */
export function validateFieldDefinition(
  field: CifFieldDefinition,
): ValidationResult {
  if (cifTypeFromString(field.type) === undefined) {
    return { ok: false, error: `Unsupported type: ${field.type}` };
  }
  if (field.required && field.default !== undefined) {
    return {
      ok: false,
      error: "Required fields should not have default values",
    };
  }
  return { ok: true };
}

/** Transformation mapping for a field. */
export interface FieldTransformation {
  source_path: string;
  /** Wire name matches Rust `#[serde(rename = "type")]`. */
  type: string;
  notes?: string;
}

export class FieldTransformationBuilder {
  private readonly t: FieldTransformation;

  constructor(sourcePath: string, targetType: string) {
    this.t = { source_path: sourcePath, type: targetType };
  }

  withNotes(notes: string): this {
    this.t.notes = notes;
    return this;
  }

  build(): FieldTransformation {
    return stripUndefined({ ...this.t });
  }
}

export function fieldTransformation(
  sourcePath: string,
  targetType: string,
): FieldTransformationBuilder {
  return new FieldTransformationBuilder(sourcePath, targetType);
}

/**
 * Schema-level helpers. The Rust port uses a `Schema` trait; in TS we pass the
 * field list directly as `Array<[name, definition]>` — simpler, equally typed,
 * and matches the JSON `Schema` which is the canonical API anyway.
 */
export type SchemaFields = ReadonlyArray<readonly [string, CifFieldDefinition]>;

/** Convert a field list to the JSON-schema envelope Rust emits. */
export function toJsonSchema(fields: SchemaFields): JsonValue {
  const obj: Record<string, JsonValue> = {};
  for (const [name, def] of fields) {
    obj[name] = {
      type: def.type,
      required: def.required,
      description: def.description ?? null,
      default: def.default ?? null,
    };
  }
  return { cif_schema: obj };
}

/** Validate a JSON value against a declared field list. */
export function validateSchema(
  fields: SchemaFields,
  value: JsonValue,
): SchemaValidationResult {
  const errors: string[] = [];
  if (!isPlainObject(value)) {
    errors.push("Value must be an object");
    return { ok: false, errors };
  }

  for (const [name, def] of fields) {
    const present = Object.prototype.hasOwnProperty.call(value, name);
    if (def.required && !present) {
      errors.push(`Missing required field: ${name}`);
    }
    if (present) {
      const fieldValue = value[name] as JsonValue;
      const r = validateFieldType(fieldValue, def.type);
      if (!r.ok) {
        errors.push(`Field '${name}': ${r.error}`);
      }
    }
  }

  return errors.length === 0 ? { ok: true } : { ok: false, errors };
}

/** Check that a JSON value matches an expected type name. */
export function validateFieldType(
  value: JsonValue,
  expectedType: string,
): ValidationResult {
  const actual = jsonValueKind(value);
  if (actual === expectedType) {
    return { ok: true };
  }
  return {
    ok: false,
    error: `Expected type '${expectedType}', got '${actual}'`,
  };
}

function jsonValueKind(value: JsonValue): CifType {
  if (value === null) return "null";
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "number") return "number";
  if (typeof value === "string") return "string";
  if (Array.isArray(value)) return "array";
  return "object";
}

function isPlainObject(
  value: JsonValue,
): value is { [k: string]: JsonValue } {
  return (
    value !== null && typeof value === "object" && !Array.isArray(value)
  );
}

function stripUndefined<T extends object>(obj: T): T {
  const record = obj as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (record[key] === undefined) {
      delete record[key];
    }
  }
  return obj;
}
