/**
 * `AncestorStore` port — interface + shared types.
 *
 * The ancestor is the last-synced canonical view of an entity. Every completed
 * sync cycle advances it *after* all pushes confirm — never before. Without
 * it, three-way diff cannot distinguish "A changed" from "both changed" and
 * silent overwrites become possible.
 *
 * This module defines the interface only. Concrete stores live in
 * `src/adapters` — an in-memory reference impl lives at
 * `adapters/inMemoryAncestor.ts`.
 *
 * # Async-by-default
 *
 * The Rust trait is synchronous, but the TS port follows plan §6: every method
 * is `async`. Future persistence adapters (Postgres, Redis, S3) will need it,
 * and changing the interface later would ripple through every call site.
 */

import type { JsonValue } from "../domain/types.js";

/**
 * Composite key for an ancestor entry.
 *
 * `entityType` keeps different canonical shapes in the same store from
 * colliding (e.g. a `PurchaseOrder` with the same id as an `InventoryItem`).
 */
export class AncestorKey {
  readonly entityType: string;
  readonly canonicalId: string;

  constructor(entityType: string, canonicalId: string) {
    this.entityType = entityType;
    this.canonicalId = canonicalId;
  }
}

/** One stored ancestor — the canonical view last confirmed on both sides. */
export class AncestorEntry {
  /**
   * The canonical JSON at last sync. This is what three-way diff compares
   * both current views against.
   */
  readonly canonical: JsonValue;

  /**
   * Milliseconds since unix epoch. The caller supplies this so tests and
   * deterministic replays can seed fixed timestamps.
   */
  readonly updatedAtMs: number;

  constructor(canonical: JsonValue, updatedAtMs: number) {
    this.canonical = canonical;
    this.updatedAtMs = updatedAtMs;
  }
}

/**
 * Errors the store may raise. Kept narrow on purpose — real persistence
 * adapters can wrap I/O failures as `SyncError.Transient` at the orchestrator
 * seam.
 *
 * Thrown rather than returned (plan §5: `Result<T, E>` for adapter/IO failures
 * is a thrown `Error` in TS so stack traces survive and `try/catch` is the
 * single failure-handling shape).
 */
export class AncestorStoreError extends Error {
  override readonly name = "AncestorStoreError";

  constructor(message: string) {
    super(`ancestor store backend failure: ${message}`);
  }
}

/**
 * Read/write interface for ancestors. All methods are async.
 *
 * Implementations MUST:
 * - return `undefined` from `get` when the key isn't stored (not throw)
 * - throw {@link AncestorStoreError} on backend failure
 * - be safe to call from a single orchestrator cycle serially; no cross-cycle
 *   concurrency guarantees required at this layer
 */
export interface AncestorStore {
  get(key: AncestorKey): Promise<AncestorEntry | undefined>;
  put(key: AncestorKey, entry: AncestorEntry): Promise<void>;
}
