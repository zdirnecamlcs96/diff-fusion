/**
 * `SyncEngine` — the facade for the reconciliation pipeline (Tier-1).
 *
 * The only type most users should need to touch. It hides the moving parts
 * of a full sync (ancestor store, escalation queue, orchestrator wiring)
 * behind a builder API. Advanced users can still reach for `Orchestrator`
 * directly.
 *
 * The facade flattens the orchestrator's richer types so users don't need
 * to import `CycleOutcome`, `UnresolvedConflict`, or `FieldChange`. No
 * internal types leak through the public surface — the Rust facade tests
 * assert this and the TS port preserves the property.
 */

import type { JsonValue } from "../domain/types.js";
import {
  InMemoryAncestorStore,
} from "../adapters/inMemoryAncestor.js";
import { InMemoryEscalationQueue } from "../adapters/inMemoryEscalation.js";
import {
  AncestorEntry,
  AncestorKey,
  type AncestorStore,
} from "../ports/ancestor.js";
import type { EscalationQueue } from "../ports/escalation.js";
import type { SystemPort } from "../ports/system.js";
import {
  type CycleOutcome,
  Orchestrator,
} from "../application/orchestrator.js";
import {
  type ConflictClass,
  type MergePolicy,
  PolicyMap,
  type UnresolvedConflict,
} from "../application/policy/index.js";
import {
  type Invariant,
  InvariantSet,
} from "../application/policy/invariants.js";
import { OwnedBy } from "../application/policy/ownedBy.js";

/**
 * What happened during a sync. The facade flattens the orchestrator's
 * richer types so users don't need to import `CycleOutcome`,
 * `UnresolvedConflict`, or `FieldChange`.
 */
export type SyncOutcome =
  | { kind: "NoOp" }
  | { kind: "Synced"; pushedTo: string[] }
  | { kind: "Escalated"; conflicts: FacadeConflict[] };

/**
 * A user-facing summary of a conflict. Carries just the fields a caller
 * needs to surface the issue to a human — no internal types.
 */
export interface FacadeConflict {
  path: string;
  reason: string;
  /**
   * Cause category — branch on this for per-class dispositions (reject vs
   * escalate vs preserve-both).
   */
  class: ConflictClass;
}

/** Preview of what a sync would do, without writing. */
export interface FacadePreview {
  /**
   * The canonical value the cycle would have written on success, or
   * `undefined` if it would have escalated.
   */
  wouldWrite: JsonValue | undefined;
  /** Conflicts the cycle would have escalated. */
  conflicts: FacadeConflict[];
}

/**
 * Reconciliation facade. Prefer the fluent path: `SyncEngine.builder(...)`.
 * Direct construction is available for advanced callers who have already
 * wired an `Orchestrator` themselves.
 */
export class SyncEngine {
  private readonly orchestrator: Orchestrator;
  private readonly escalation: EscalationQueue;

  constructor(orchestrator: Orchestrator, escalation: EscalationQueue) {
    this.orchestrator = orchestrator;
    this.escalation = escalation;
  }

  /** Start building an engine from two adapters. */
  static builder(sideA: SystemPort, sideB: SystemPort): SyncEngineBuilder {
    return new SyncEngineBuilder(sideA, sideB);
  }

  /**
   * Run one reconciliation cycle for the given entity. Uses the system
   * clock for the ancestor timestamp.
   */
  async sync(
    entityType: string,
    canonicalId: string,
  ): Promise<SyncOutcome> {
    const out = await this.orchestrator.runCycleAt(
      entityType,
      canonicalId,
      Date.now(),
    );
    return flatten(out);
  }

  /**
   * Preview what `sync` would do, without writing or advancing the
   * ancestor. The equivalent of a dry run.
   */
  async preview(
    entityType: string,
    canonicalId: string,
  ): Promise<FacadePreview> {
    const report = await this.orchestrator.runShadow(entityType, canonicalId);
    return {
      wouldWrite: report.wouldWrite,
      conflicts: toFacadeConflicts(report.resolution.conflicts),
    };
  }

  /** How many items currently sit in the escalation queue. */
  async escalationDepth(): Promise<number> {
    return this.escalation.len();
  }
}

/** Builder for `SyncEngine`. Use `SyncEngine.builder(...)` to obtain one. */
export class SyncEngineBuilder {
  private readonly sideA: SystemPort;
  private readonly sideB: SystemPort;
  private policies = new PolicyMap();
  private invariants = new InvariantSet();
  private ancestor: AncestorStore | undefined;
  /**
   * Concrete handle for the default ancestor store, used by `seedAncestor`.
   * `undefined` when the caller supplied a custom store.
   */
  private defaultAncestor: InMemoryAncestorStore | undefined;
  private escalation: EscalationQueue | undefined;

  constructor(sideA: SystemPort, sideB: SystemPort) {
    this.sideA = sideA;
    this.sideB = sideB;
  }

  /** Install a per-path merge policy. */
  policy(path: string, policy: MergePolicy): this {
    this.policies.with(path, policy);
    return this;
  }

  /** Install a fallback merge policy for paths not otherwise covered. */
  defaultPolicy(policy: MergePolicy): this {
    this.policies.withDefault(policy);
    return this;
  }

  /** Add a Tier-2 post-merge invariant. */
  invariant(invariant: Invariant): this {
    this.invariants.with(invariant);
    return this;
  }

  /**
   * One-way sync preset: `sideA` becomes the source of truth; any field
   * not overridden by a subsequent `policy(...)` call is owned by
   * `sideA`. Target-side edits revert on the next cycle
   * (Synology-style "download only" semantics).
   */
  oneWay(): this {
    this.policies = new PolicyMap().withDefault(new OwnedBy(this.sideA.systemType()));
    return this;
  }

  /**
   * Supply a custom ancestor store. Without this call the engine uses an
   * in-memory default (suitable for tests; not durable).
   */
  ancestorStore(store: AncestorStore): this {
    this.ancestor = store;
    this.defaultAncestor = undefined;
    return this;
  }

  /**
   * Supply a custom escalation queue. Without this call the engine uses
   * an in-memory default.
   */
  escalationQueue(queue: EscalationQueue): this {
    this.escalation = queue;
    return this;
  }

  /**
   * Pre-populate the (default, in-memory) ancestor store with a known
   * baseline. Useful in tests and for explicit initial-sync seeding. No
   * effect when a custom ancestor store was supplied.
   */
  seedAncestor(
    entityType: string,
    canonicalId: string,
    canonical: JsonValue,
  ): this {
    const store = this.ensureDefaultAncestor();
    // InMemoryAncestorStore.put is async but the in-memory impl resolves
    // synchronously. Calling it and discarding the promise is safe here
    // because the backing Map is mutated before the promise resolves.
    void store.put(
      new AncestorKey(entityType, canonicalId),
      new AncestorEntry(canonical, 0),
    );
    return this;
  }

  private ensureDefaultAncestor(): InMemoryAncestorStore {
    if (this.defaultAncestor !== undefined) return this.defaultAncestor;
    const fresh = new InMemoryAncestorStore();
    this.defaultAncestor = fresh;
    this.ancestor = fresh;
    return fresh;
  }

  /**
   * Validate the installed policies against a CIF schema before the first
   * cycle runs. For each registered policy, the field at
   * `schema.cif_schema.<path>` is inspected — `SetByKey` verifies that its
   * `a_anchor` / `b_anchor` fields exist with the matching `anchor` roles,
   * its `identity` fields exist, and any `nested` policies line up with
   * declared sub-arrays.
   *
   * Returns `{ ok: true }` on success or `{ ok: false; errors }` on any
   * misalignment. Each error is prefixed with the policy path. Call before
   * `build()` to fail fast rather than at first cycle.
   */
  validateAgainstSchema(
    schema: JsonValue,
  ): { ok: true } | { ok: false; errors: string[] } {
    const errors = this.policies.validateAgainstSchema(schema);
    if (errors.length === 0) return { ok: true };
    return { ok: false, errors };
  }

  build(): SyncEngine {
    const ancestor = this.ancestor ?? new InMemoryAncestorStore();
    const escalation = this.escalation ?? new InMemoryEscalationQueue();

    const orchestrator = new Orchestrator(
      this.sideA,
      this.sideB,
      ancestor,
      this.policies,
      escalation,
    ).withInvariants(this.invariants);

    return new SyncEngine(orchestrator, escalation);
  }
}

// ---------------------------------------------------------------------------
// Internals — outcome flattening (no internal types leak)
// ---------------------------------------------------------------------------

function flatten(out: CycleOutcome): SyncOutcome {
  switch (out.kind) {
    case "NoOp":
      return { kind: "NoOp" };
    case "Synced":
      return { kind: "Synced", pushedTo: out.pushedTo };
    case "Escalated":
      return {
        kind: "Escalated",
        conflicts: toFacadeConflicts(out.conflicts),
      };
    default: {
      const _exhaustive: never = out;
      throw new Error(
        `unreachable CycleOutcome: ${JSON.stringify(_exhaustive)}`,
      );
    }
  }
}

function toFacadeConflicts(
  conflicts: readonly UnresolvedConflict[],
): FacadeConflict[] {
  return conflicts.map((c) => ({
    path: c.path,
    reason: c.reason,
    class: c.class,
  }));
}
