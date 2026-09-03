/**
 * Typed wrapper over the Rust WASM kernel. The only file allowed to touch
 * the wasm module. Boundary contract: JSON string in → JSON string out.
 *
 * The wasm kernel speaks the Rust wire format (snake_case field names,
 * explicit `null` for absent values). The rest of this codebase speaks the
 * idiomatic TS `FieldChange` shape (camelCase, `undefined` for absent
 * values). This file is where that translation happens — every other
 * caller only ever sees the TS shape.
 */
import { createRequire } from "node:module";
import { z } from "zod";
import { jsonValueSchema, type JsonValue } from "./domain/types.js";
import type { Changelog, FieldChange } from "./domain/diff/threeWay.js";
import {
  mergeOutcomeSchema,
  type MergeContext,
  type MergeOutcome,
} from "./application/policy/index.js";
import type {
  MergePolicyRef,
  PolicyDocument,
} from "./application/policy/declaration.js";
import type { CompareChange } from "./domain/compare.js";
import type { TransformResult } from "./application/transform.js";
import {
  wireChangelogSchema,
  wireFieldChangeSchema,
  type WireChangelog,
  type WireFieldChange,
} from "./generated/wireChangelog.js";

/**
 * Wire shape for a `FieldChange`, as produced/consumed by the Rust kernel.
 *
 * Wire contract: an ABSENT `new_from_a`/`new_from_b` key means that side
 * didn't touch the field (unchanged); a PRESENT key — including `null` —
 * means it changed, possibly to `null` (a legitimate clear). Never use
 * presence-collapsing (`?? null` / `?? undefined`) to translate these
 * fields: that conflates "didn't touch" with "cleared to null", which is
 * the exact defect this wire shape exists to avoid.
 */
export { wireChangelogSchema, wireFieldChangeSchema };

const require = createRequire(import.meta.url);
// ponytail: sync CJS load via createRequire; revisit if a browser build is needed
const wasm = require("../wasm/diff_fusion.js");

function fieldChangeFromWire(w: WireFieldChange): FieldChange {
  return {
    path: w.path,
    oldValue: w.old_value,
    newFromA: "new_from_a" in w ? w.new_from_a : undefined,
    newFromB: "new_from_b" in w ? w.new_from_b : undefined,
    source: w.source,
  };
}

function fieldChangeToWire(c: FieldChange): WireFieldChange {
  const wire: WireFieldChange = {
    path: c.path,
    old_value: c.oldValue,
    source: c.source,
  };
  if (c.newFromA !== undefined) wire.new_from_a = c.newFromA;
  if (c.newFromB !== undefined) wire.new_from_b = c.newFromB;
  return wire;
}

export function kernelThreeWayDiff(
  ancestor: JsonValue,
  a: JsonValue,
  b: JsonValue,
): Changelog {
  const raw: WireChangelog = wireChangelogSchema.parse(
    JSON.parse(
      wasm.three_way_diff(
        JSON.stringify(ancestor),
        JSON.stringify(a),
        JSON.stringify(b),
      ),
    ),
  );
  return { changes: raw.changes.map(fieldChangeFromWire) };
}

export function kernelMergeField(
  change: FieldChange,
  ref: MergePolicyRef,
  ctx: MergeContext,
): MergeOutcome {
  return mergeOutcomeSchema.parse(
    JSON.parse(
      wasm.merge_field(
        JSON.stringify(fieldChangeToWire(change)),
        JSON.stringify(ref),
        JSON.stringify({ system_a: ctx.system_a, system_b: ctx.system_b }),
      ),
    ),
  );
}

/**
 * Raw string passthrough for `three_way_diff` / `merge_field`, bypassing the
 * JSON.stringify/parse the typed wrappers above do. For conformance tests
 * that must hand the wasm boundary byte-identical input shared across
 * runtimes, rather than a value re-encoded by `JSON.stringify`.
 */
export function kernelThreeWayDiffRaw(
  ancestor: string,
  a: string,
  b: string,
): string {
  return wasm.three_way_diff(ancestor, a, b);
}

export function kernelMergeFieldRaw(
  change: string,
  ref: string,
  ctx: string,
): string {
  return wasm.merge_field(change, ref, ctx);
}

/**
 * Shared shape for one unresolved conflict, as emitted by both `merge_batch`
 * and `fuse`. `class` mirrors the Rust enum's snake_case variants; `change`
 * is the wire `FieldChange` (absent-vs-null significant), left untranslated
 * since no domain-level conflict type exists yet for callers to consume.
 */
const kernelConflictSchema = z.object({
  path: z.string(),
  class: z.enum(["no_policy", "policy_conflict", "invariant_violation"]),
  reason: z.string(),
  change: wireFieldChangeSchema,
});
export type KernelConflict = z.infer<typeof kernelConflictSchema>;

export const batchResolutionSchema = z.object({
  resolved: z.array(z.object({ path: z.string(), value: jsonValueSchema })),
  conflicts: z.array(kernelConflictSchema),
});
export type BatchResolution = z.infer<typeof batchResolutionSchema>;

export function kernelMergeBatch(
  changelog: Changelog,
  policyDoc: PolicyDocument,
  ctx: MergeContext,
): BatchResolution {
  return batchResolutionSchema.parse(
    JSON.parse(
      wasm.merge_batch(
        JSON.stringify({ changes: changelog.changes.map(fieldChangeToWire) }),
        JSON.stringify(policyDoc),
        JSON.stringify({ system_a: ctx.system_a, system_b: ctx.system_b }),
      ),
    ),
  );
}

/**
 * Raw string passthrough for `merge_batch`, bypassing the JSON.stringify/parse
 * the typed wrapper above does. For conformance tests that must hand the wasm
 * boundary byte-identical input shared across runtimes, mirrors
 * `kernelMergeFieldRaw`.
 */
export function kernelMergeBatchRaw(
  changelog: string,
  policyDoc: string,
  ctx: string,
): string {
  return wasm.merge_batch(changelog, policyDoc, ctx);
}

export const fuseResultSchema = z.object({
  value: jsonValueSchema,
  conflicts: z.array(kernelConflictSchema),
});
export type FuseResult = z.infer<typeof fuseResultSchema>;

export function kernelFuse(
  ancestor: JsonValue,
  a: JsonValue,
  b: JsonValue,
  policyDoc: PolicyDocument,
  ctx: MergeContext,
): FuseResult {
  return fuseResultSchema.parse(
    JSON.parse(
      wasm.fuse(
        JSON.stringify(ancestor),
        JSON.stringify(a),
        JSON.stringify(b),
        JSON.stringify(policyDoc),
        JSON.stringify({ system_a: ctx.system_a, system_b: ctx.system_b }),
      ),
    ),
  );
}

/**
 * Raw string passthrough for `fuse`, bypassing the JSON.stringify/parse the
 * typed wrapper above does. For conformance tests that must hand the wasm
 * boundary byte-identical input shared across runtimes, mirrors
 * `kernelMergeBatchRaw`.
 */
export function kernelFuseRaw(
  ancestor: string,
  a: string,
  b: string,
  policyDoc: string,
  ctx: string,
): string {
  return wasm.fuse(ancestor, a, b, policyDoc, ctx);
}

const compareChangeSchema = z.tuple([
  z.string(),
  z.tuple([jsonValueSchema, jsonValueSchema]),
]);

export function kernelCompareJson(a: JsonValue, b: JsonValue): CompareChange[] {
  return z
    .array(compareChangeSchema)
    .parse(JSON.parse(wasm.compare_json(JSON.stringify(a), JSON.stringify(b))));
}

/**
 * Raw string passthrough for `compare_json`, bypassing the JSON.stringify/parse
 * the typed wrapper above does. For conformance tests that must hand the wasm
 * boundary byte-identical input shared across runtimes, mirrors
 * `kernelThreeWayDiffRaw`.
 */
export function kernelCompareJsonRaw(a: string, b: string): string {
  return wasm.compare_json(a, b);
}

/**
 * Wasm target throws on failure (no ok/err envelope on this target); this
 * is where that converts back to TS's discriminated-union `TransformResult`.
 */
export function kernelTransformToCif(
  source: JsonValue,
  schema: JsonValue,
  formatId: string,
): TransformResult {
  try {
    const raw = wasm.transform_to_cif(
      JSON.stringify(source),
      JSON.stringify(schema),
      formatId,
    );
    return { ok: true, value: jsonValueSchema.parse(JSON.parse(raw)) };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

/**
 * Raw string passthrough for `transform_to_cif`, bypassing the try/catch
 * envelope conversion `kernelTransformToCif` does. For conformance tests
 * that must hand the wasm boundary byte-identical input shared across
 * runtimes, mirrors `kernelCompareJsonRaw`.
 */
export function kernelTransformToCifRaw(
  source: string,
  schema: string,
  formatId: string,
): string {
  return wasm.transform_to_cif(source, schema, formatId);
}

/**
 * Wasm target throws on failure (no ok/err envelope on this target); this
 * is where that converts back to TS's discriminated-union `TransformResult`.
 */
export function kernelTransformFromCif(
  cif: JsonValue,
  schema: JsonValue,
  formatId: string,
): TransformResult {
  try {
    const raw = wasm.transform_from_cif(
      JSON.stringify(cif),
      JSON.stringify(schema),
      formatId,
    );
    return { ok: true, value: jsonValueSchema.parse(JSON.parse(raw)) };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

/**
 * Raw string passthrough for `transform_from_cif`, bypassing the try/catch
 * envelope conversion `kernelTransformFromCif` does. For conformance tests
 * that must hand the wasm boundary byte-identical input shared across
 * runtimes, mirrors `kernelTransformToCifRaw`.
 */
export function kernelTransformFromCifRaw(
  cif: string,
  schema: string,
  formatId: string,
): string {
  return wasm.transform_from_cif(cif, schema, formatId);
}

export function kernelResolve(
  ancestor: JsonValue,
  changelog: Changelog,
  policyDoc: PolicyDocument,
  ctx: MergeContext,
): FuseResult {
  return fuseResultSchema.parse(
    JSON.parse(
      wasm.resolve(
        JSON.stringify(ancestor),
        JSON.stringify({ changes: changelog.changes.map(fieldChangeToWire) }),
        JSON.stringify(policyDoc),
        JSON.stringify({ system_a: ctx.system_a, system_b: ctx.system_b }),
      ),
    ),
  );
}

/**
 * Raw string passthrough for `resolve`, bypassing the JSON.stringify/parse the
 * typed wrapper above does. For conformance tests that must hand the wasm
 * boundary byte-identical input shared across runtimes, mirrors `kernelFuseRaw`.
 */
export function kernelResolveRaw(
  ancestor: string,
  changelog: string,
  policyDoc: string,
  ctx: string,
): string {
  return wasm.resolve(ancestor, changelog, policyDoc, ctx);
}

export function kernelCanonicalJson(doc: JsonValue): string {
  return wasm.canonical_json(JSON.stringify(doc));
}

export function kernelIdempotencyKeyHex(
  canonicalId: string,
  operation: string,
  payload: JsonValue,
): string {
  return wasm.idempotency_key_hex(
    canonicalId,
    operation,
    JSON.stringify(payload),
  );
}
