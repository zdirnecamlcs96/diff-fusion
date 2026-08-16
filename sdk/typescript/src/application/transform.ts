/**
 * Schema-driven conversion of source JSON into the Common Intermediate
 * Format (CIF). Delegates to the Rust WASM kernel (`kernelTransformToCif`
 * in `../kernel.js`) — the schema-walking logic is SSOT there.
 *
 * The schema is a full JSON document of shape:
 *   { cif_schema: { <cif_field>: { type, required?, element?, children? } },
 *     transformations: { <format_id>: { <cif_field>: { source_path, type, element?, children? } } } }
 *
 * Returns a discriminated union rather than throwing, matching the
 * in-domain convention in this codebase (see §5 of TS_PORT_PLAN.md).
 */

import { kernelTransformToCif } from "../kernel.js";
import type { JsonValue } from "../domain/types.js";

export type TransformResult =
  | { ok: true; value: JsonValue }
  | { ok: false; error: string };

export type TransformStringResult =
  | { ok: true; value: string }
  | { ok: false; error: string };

/** Transform a JSON document to CIF using `schema.transformations[formatId]`. */
export function toCif(
  source: JsonValue,
  schema: JsonValue,
  formatId: string,
): TransformResult {
  return kernelTransformToCif(source, schema, formatId);
}

/** Backward-compatible free-function alias for `toCif`. */
export function transformToCif(
  source: JsonValue,
  schema: JsonValue,
  formatId: string,
): TransformResult {
  return toCif(source, schema, formatId);
}

/** Stringified entry point: parse inputs, transform, re-stringify output. */
export function toCifString(
  sourceJson: string,
  schemaJson: string,
  formatId: string,
): TransformStringResult {
  let source: JsonValue;
  try {
    source = JSON.parse(sourceJson) as JsonValue;
  } catch (e) {
    return { ok: false, error: `Invalid source JSON: ${describe(e)}` };
  }
  let schema: JsonValue;
  try {
    schema = JSON.parse(schemaJson) as JsonValue;
  } catch (e) {
    return { ok: false, error: `Invalid schema JSON: ${describe(e)}` };
  }
  const r = toCif(source, schema, formatId);
  if (!r.ok) return r;
  return { ok: true, value: JSON.stringify(r.value) };
}

function describe(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
