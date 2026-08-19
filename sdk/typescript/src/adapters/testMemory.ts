/**
 * In-memory reference adapter.
 *
 * Used by the shared contract test suite and any integration test that
 * needs a {@link SystemPort} without hitting a network. Two instances with
 * different `systemType` labels simulate a two-system sync end to end.
 *
 * Behavior:
 * - Storage is a nested Map: entityType → externalId → entity.
 * - Reverse lookup uses a secondary entityType → canonicalId → externalId index.
 * - Version strings are monotonic decimal counters (`"1"`, `"2"`, ...).
 * - OCC is enforced: an upsert with a stale `expectVersion` throws a
 *   {@link SyncError} with `kind: "StaleWrite"`.
 * - Idempotency keys are tracked per entity: a repeat upsert with the same
 *   key is a no-op, returning the existing ref unchanged.
 * - `upsert` shallow-merges the pushed CIF's top-level fields onto any
 *   existing stored record — it never replaces the record wholesale, so
 *   fields the CIF mapping doesn't carry (native-only state) survive a
 *   push. See `ports/conformance.ts`.
 */

import { SyncError } from "../domain/error.js";
import type { JsonValue } from "../domain/types.js";
import { ExternalRef, type SystemPort } from "../ports/system.js";

interface StoredEntity {
  externalId: string;
  canonical: JsonValue;
  version: number;
  lastIdempotencyKey: Uint8Array | undefined;
}

function keyEquals(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function getNested<V>(m: Map<string, Map<string, V>>, a: string, b: string): V | undefined {
  return m.get(a)?.get(b);
}

function setNested<V>(m: Map<string, Map<string, V>>, a: string, b: string, v: V): void {
  let inner = m.get(a);
  if (inner === undefined) {
    inner = new Map();
    m.set(a, inner);
  }
  inner.set(b, v);
}

function isJsonObject(v: JsonValue): v is { [k: string]: JsonValue } {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Shallow-merge `patch`'s top-level keys onto `base`. Non-object values
 * fall back to a straight replace (there's no sensible field to merge).
 */
function shallowMerge(base: JsonValue, patch: JsonValue): JsonValue {
  if (!isJsonObject(base) || !isJsonObject(patch)) return patch;
  return { ...base, ...patch };
}

export class TestMemoryAdapter implements SystemPort {
  private readonly _systemType: string;
  private readonly entities: Map<string, Map<string, StoredEntity>> = new Map();
  private readonly canonicalIndex: Map<string, Map<string, string>> = new Map();
  private nextExternalId = 0;
  private nextVersion = 0;

  constructor(systemType: string) {
    this._systemType = systemType;
  }

  /**
   * Seed an entity directly. Bypasses the normal upsert path, useful for
   * tests that want to start the adapter in a specific state.
   */
  seed(entityType: string, canonicalId: string, canonical: JsonValue): ExternalRef {
    this.nextExternalId += 1;
    this.nextVersion += 1;
    const externalId = `EXT-${this.nextExternalId}`;
    const version = this.nextVersion;
    const stored: StoredEntity = {
      externalId,
      canonical,
      version,
      lastIdempotencyKey: undefined,
    };
    setNested(this.entities, entityType, externalId, stored);
    setNested(this.canonicalIndex, entityType, canonicalId, externalId);
    return new ExternalRef(this._systemType, externalId, String(version));
  }

  systemType(): string {
    return this._systemType;
  }

  private makeRef(externalId: string, version: number): ExternalRef {
    return new ExternalRef(this._systemType, externalId, String(version));
  }

  async fetch(
    entityType: string,
    ext: ExternalRef,
  ): Promise<{ canonical: JsonValue; ref: ExternalRef }> {
    const e = getNested(this.entities, entityType, ext.externalId);
    if (e === undefined) {
      throw SyncError.transient(`entity not found: ${entityType}/${ext.externalId}`);
    }
    return { canonical: e.canonical, ref: this.makeRef(e.externalId, e.version) };
  }

  async findByCanonicalId(
    entityType: string,
    canonicalId: string,
  ): Promise<ExternalRef | undefined> {
    const externalId = getNested(this.canonicalIndex, entityType, canonicalId);
    if (externalId === undefined) return undefined;
    const e = getNested(this.entities, entityType, externalId);
    if (e === undefined) return undefined;
    return this.makeRef(e.externalId, e.version);
  }

  async upsert(
    entityType: string,
    canonicalId: string,
    canonical: JsonValue,
    expectVersion: string | undefined,
    idempotencyKey: Uint8Array,
  ): Promise<ExternalRef> {
    const existingExtId = getNested(this.canonicalIndex, entityType, canonicalId);
    let existingCanonical: JsonValue | undefined;

    if (existingExtId !== undefined) {
      const existing = getNested(this.entities, entityType, existingExtId);
      if (existing === undefined) {
        throw SyncError.transient("index points to missing entity");
      }

      if (
        existing.lastIdempotencyKey !== undefined &&
        keyEquals(existing.lastIdempotencyKey, idempotencyKey)
      ) {
        return this.makeRef(existing.externalId, existing.version);
      }

      if (expectVersion !== undefined && expectVersion !== String(existing.version)) {
        throw SyncError.staleWrite({
          system: this._systemType,
          expected: String(existing.version),
          message: `version mismatch: caller expected ${expectVersion}, current ${existing.version}`,
        });
      }

      existingCanonical = existing.canonical;
    } else if (expectVersion !== undefined) {
      throw SyncError.staleWrite({
        system: this._systemType,
        message: "caller supplied expect_version but no existing record",
      });
    }

    this.nextVersion += 1;
    const version = this.nextVersion;

    let externalId: string;
    if (existingExtId !== undefined) {
      externalId = existingExtId;
    } else {
      this.nextExternalId += 1;
      externalId = `EXT-${this.nextExternalId}`;
      setNested(this.canonicalIndex, entityType, canonicalId, externalId);
    }

    // Read-modify-write: shallow-merge the pushed CIF onto whatever's
    // already stored, so fields outside the CIF mapping (native-only local
    // state) survive the push instead of being wiped by replace.
    // ponytail: top-level shallow merge; deep path-merge if nested local fields ever needed
    const mergedCanonical =
      existingCanonical !== undefined ? shallowMerge(existingCanonical, canonical) : canonical;

    const stored: StoredEntity = {
      externalId,
      canonical: mergedCanonical,
      version,
      lastIdempotencyKey: idempotencyKey,
    };
    setNested(this.entities, entityType, externalId, stored);

    return this.makeRef(externalId, version);
  }
}
