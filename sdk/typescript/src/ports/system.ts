/**
 * `SystemPort` — the seam between the orchestrator and any single external
 * system.
 *
 * Every system (ERP, internal service, third-party API) implements the same
 * interface. Code above the port never branches on system identity; per-system
 * quirks are hidden inside the adapter.
 *
 * # Canonical-only
 *
 * The interface deals exclusively in canonical JSON values. Bidirectional
 * transformation (external shape ↔ canonical) is the adapter's private
 * concern. This keeps the orchestrator format-agnostic.
 *
 * # Optimistic concurrency
 *
 * Every {@link SystemPort.upsert} takes an `expectVersion`. If the external
 * system moved since the orchestrator last fetched, the adapter throws a
 * {@link SyncError} with `kind: "StaleWrite"` and the cycle restarts. Adapters
 * against systems without native OCC fake it with a read-before-write check.
 *
 * # Idempotency
 *
 * Every upsert takes a deterministic 32-byte idempotency key (see
 * `domain/idempotency.ts`). Adapters must forward it to the external system's
 * idempotency mechanism when supported, or maintain their own dedup table
 * when not.
 */

import type { JsonValue } from "../domain/types.js";

/**
 * Identifies an entity in an external system.
 *
 * `version` is whatever the external system uses to detect concurrent
 * modifications — a revision number, an HTTP ETag, a commit hash. It is
 * opaque to the orchestrator; only the adapter knows how to compare them.
 */
export class ExternalRef {
  readonly system: string;
  readonly externalId: string;
  readonly version: string | undefined;

  constructor(system: string, externalId: string, version: string | undefined) {
    this.system = system;
    this.externalId = externalId;
    this.version = version;
  }
}

/**
 * The adapter interface.
 *
 * Methods throw {@link SyncError} (from `domain/error.ts`) on adapter/IO
 * failures: `Transient` for retriable blips, `StaleWrite` when `expectVersion`
 * mismatches, `Conflict` for hard contract violations. Successful calls return
 * data directly.
 */
export interface SystemPort {
  /**
   * Stable identifier for this system — used in logs, policy contexts, and
   * {@link ExternalRef.system}.
   */
  systemType(): string;

  /** Fetch the current canonical view for the given external ref. */
  fetch(entityType: string, ext: ExternalRef): Promise<{
    canonical: JsonValue;
    ref: ExternalRef;
  }>;

  /**
   * Reverse lookup: find the external ref for a canonical id.
   *
   * This is the "findByCanonicalId" method whose absence causes most of the
   * duplicate-record class of bugs — when a webhook fires mid-cycle and the
   * adapter can't tell whether the entity already exists externally, it
   * creates a second record.
   */
  findByCanonicalId(
    entityType: string,
    canonicalId: string,
  ): Promise<ExternalRef | undefined>;

  /**
   * Upsert canonical state. Resolves to the new {@link ExternalRef} (with the
   * post-write version). The `idempotencyKey` is supplied by the orchestrator
   * via `idempotencyKey()` from `domain/idempotency.ts`.
   *
   * `expectVersion` is the orchestrator's assertion about the version it last
   * saw. When it mismatches, adapters MUST throw `SyncError` with
   * `kind: "StaleWrite"` rather than silently overwriting.
   *
   * `idempotencyKey` is always a 32-byte `Uint8Array` — enforced at runtime by
   * the orchestrator; adapters can rely on the length.
   */
  upsert(
    entityType: string,
    canonicalId: string,
    canonical: JsonValue,
    expectVersion: string | undefined,
    idempotencyKey: Uint8Array,
  ): Promise<ExternalRef>;
}
