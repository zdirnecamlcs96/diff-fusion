/**
 * `EscalationQueue` port — where unresolved conflicts go for human review.
 *
 * App.md § 03 is explicit: ~5% of conflicts cannot be auto-resolved, and
 * throwing them away (or silently picking a winner) is worse than not
 * reconciling at all. The orchestrator routes these to an
 * {@link EscalationQueue} with full provenance so a reviewer can see *exactly*
 * what each side claimed and choose.
 *
 * This module defines the interface only. Concrete queues live in
 * `src/adapters` — the in-memory reference impl is at
 * `adapters/inMemoryEscalation.ts`.
 *
 * Async-by-default: see `ports/ancestor.ts` for the rationale.
 */

import type { UnresolvedConflict } from "../application/policy/index.js";

/**
 * One pending escalation. Carries the full list of unresolved conflicts so
 * the reviewer has provenance for every disputed field.
 */
export class EscalationItem {
  readonly entityType: string;
  readonly canonicalId: string;
  readonly conflicts: readonly UnresolvedConflict[];
  readonly createdAtMs: number;

  constructor(
    entityType: string,
    canonicalId: string,
    conflicts: readonly UnresolvedConflict[],
    createdAtMs: number,
  ) {
    this.entityType = entityType;
    this.canonicalId = canonicalId;
    this.conflicts = conflicts;
    this.createdAtMs = createdAtMs;
  }
}

/** Thrown by queues on backend failure (disk full, network, etc.). */
export class EscalationError extends Error {
  override readonly name = "EscalationError";

  constructor(message: string) {
    super(`escalation queue backend failure: ${message}`);
  }
}

/**
 * Read/write interface for the escalation queue.
 *
 * `len()` and `isEmpty()` are async so adapters backed by remote stores don't
 * need to maintain a local cache just to answer them.
 */
export interface EscalationQueue {
  push(item: EscalationItem): Promise<void>;
  len(): Promise<number>;
  isEmpty(): Promise<boolean>;
}
