/**
 * Conformance harness for {@link SystemPort} implementations.
 *
 * `tests/contract/systemPortContract.ts` already exercises adapters from
 * inside this package's own test suite, but it isn't shipped in `dist` — an
 * out-of-tree adapter package can't import it. This module is production
 * code so any `SystemPort` implementor can assert the same contract from
 * its own test suite.
 *
 * The port speaks CIF only (see the `SystemPort` module docs' "Canonical-
 * only" section): `upsert` receives a document containing just the fields
 * the two systems share. Fields the CIF mapping never sees are per-system
 * local state, and `upsert` must read-modify-write the shared paths onto
 * the existing native record rather than replace it wholesale — otherwise
 * a push silently deletes native-only data. Checking that needs a peek at
 * the adapter's native storage the port itself never exposes, hence
 * {@link RawAccess}: a tiny side door the adapter's own test supplies.
 *
 * Port of `core/src/ports/conformance.rs`.
 */

import { SyncError } from "../domain/error.js";
import { idempotencyKey } from "../domain/idempotency.js";
import type { JsonValue } from "../domain/types.js";
import type { ExternalRef, SystemPort } from "./system.js";

/**
 * Adapter-supplied side door into native storage, bypassing the CIF-only
 * `SystemPort` surface. Only needed for the preservation check (C1) — the
 * other checks drive the port directly.
 */
export interface RawAccess {
  /**
   * Seed a raw native record directly in the adapter's backing store,
   * bypassing `upsert`. `native` may contain fields the CIF mapping never
   * touches — the harness uses that to prove a push doesn't clobber them.
   */
  seedRaw(entityType: string, canonicalId: string, native: JsonValue): Promise<ExternalRef>;

  /**
   * Read the native record back, bypassing whatever CIF projection the
   * port applies on `fetch`.
   */
  readRaw(entityType: string, canonicalId: string): Promise<JsonValue>;
}

/**
 * Assert a {@link SystemPort} implementation honors the port's documented
 * contract. Throws (with a message naming the violated rule) on the first
 * failing check.
 */
export async function assertSystemPortContract(port: SystemPort, raw: RawAccess): Promise<void> {
  await assertC1Preservation(port, raw);
  await assertC2VersionGuard(port);
  await assertC3Idempotency(port);
}

/**
 * C1 — preservation. `upsert` must update the shared CIF paths on the
 * existing native record and leave everything else alone (read-modify-
 * write, not replace) — see this module's doc comment.
 */
async function assertC1Preservation(port: SystemPort, raw: RawAccess): Promise<void> {
  const entityType = "conformance_entity";
  const canonicalId = "conformance-c1";
  const seeded = await raw.seedRaw(entityType, canonicalId, {
    shared: "old",
    native_only: "keep-me",
  });

  const cif: JsonValue = { shared: "new" };
  const ik = idempotencyKey(canonicalId, "upsert", cif);
  await port.upsert(entityType, canonicalId, cif, seeded.version, ik);

  const after = await raw.readRaw(entityType, canonicalId);
  const afterObj = after as Record<string, JsonValue>;
  if (afterObj.shared !== "new") {
    throw new Error("C1 preservation violated: upsert must update the shared path it was given");
  }
  if (afterObj.native_only !== "keep-me") {
    throw new Error(
      "C1 preservation violated: upsert must read-modify-write the native record, " +
        "not replace it — a native-only field vanished after a push",
    );
  }
}

/**
 * C2 — version guard. The port's "Optimistic concurrency" docs promise a
 * mismatched `expectVersion` throws `SyncError` with `kind: "StaleWrite"`
 * rather than silently overwriting, so every `SystemPort` must honor it —
 * this check always runs (no skip condition).
 */
async function assertC2VersionGuard(port: SystemPort): Promise<void> {
  const entityType = "conformance_entity";
  const canonicalId = "conformance-c2";
  const cif: JsonValue = { shared: "v1" };
  const ik1 = idempotencyKey(canonicalId, "upsert", cif);
  const created = await port.upsert(entityType, canonicalId, cif, undefined, ik1);

  const stale = `stale-${created.version ?? "none"}`;
  const cif2: JsonValue = { shared: "v2" };
  const ik2 = idempotencyKey(canonicalId, "upsert", cif2);

  let threw: unknown;
  try {
    await port.upsert(entityType, canonicalId, cif2, stale, ik2);
  } catch (e) {
    threw = e;
  }
  if (threw === undefined) {
    throw new Error(
      "C2 version guard violated: upsert with a mismatched expectVersion must be rejected, not silently applied",
    );
  }
  if (!(threw instanceof SyncError) || threw.kind !== "StaleWrite") {
    throw new Error(
      `C2 version guard violated: expected SyncError with kind "StaleWrite" on version mismatch, got ${String(threw)}`,
    );
  }
}

/**
 * C3 — idempotency. The port's "Idempotency" docs require adapters to
 * dedup a repeated `idempotencyKey` (via the external system's mechanism
 * or a table of their own), so every `SystemPort` must honor it — this
 * check always runs (no skip condition).
 */
async function assertC3Idempotency(port: SystemPort): Promise<void> {
  const entityType = "conformance_entity";
  const canonicalId = "conformance-c3";
  const cif: JsonValue = { shared: "same" };
  const ik = idempotencyKey(canonicalId, "upsert", cif);

  const r1 = await port.upsert(entityType, canonicalId, cif, undefined, ik);
  const r2 = await port.upsert(entityType, canonicalId, cif, undefined, ik);

  if (r1.system !== r2.system || r1.externalId !== r2.externalId || r1.version !== r2.version) {
    throw new Error(
      "C3 idempotency violated: a repeat upsert with the same idempotency key must be a no-op, not create a new write",
    );
  }
}
