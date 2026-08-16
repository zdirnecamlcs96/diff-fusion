/// Helpers for loading a saved Capture into the DemoForm. Captures carry
/// post-transformation CIF, so we synthesise an identity transformer and
/// a matching schema on the fly so the user can click "Run Sync ▶"
/// without rewriting the form.

import type { Capture, Json } from "./types";

function inferType(v: Json): string {
  if (v === null || v === undefined) return "string";
  if (Array.isArray(v)) return "array";
  if (typeof v === "object") return "object";
  if (typeof v === "boolean") return "boolean";
  if (typeof v === "number") return "number";
  return "string";
}

/** Build a CIF schema covering the top-level keys present on either side. */
export function schemaFromCapture(c: Capture): Json {
  const a = (c.side_a.canonical_view ?? {}) as Record<string, Json>;
  const b = (c.side_b.canonical_view ?? {}) as Record<string, Json>;
  const keys = new Set<string>([...Object.keys(a), ...Object.keys(b)]);
  const cif_schema: Record<string, Json> = {};
  for (const k of [...keys].sort()) {
    const sample = a[k] !== undefined ? a[k] : b[k];
    cif_schema[k] = { type: inferType(sample), required: false };
  }
  return { cif_schema };
}

/** Identity transformer: each top-level CIF key reads from the same key in
 *  the source. Sufficient for the shallow shapes the observer typically
 *  ships; nested arrays of objects still need user tweaking. */
export function identityTransformerFor(cif: Json): Json {
  if (!cif || typeof cif !== "object" || Array.isArray(cif)) return {};
  const out: Record<string, Json> = {};
  for (const [k, v] of Object.entries(cif as Record<string, Json>)) {
    out[k] = { source_path: k, type: inferType(v) };
  }
  return out;
}
