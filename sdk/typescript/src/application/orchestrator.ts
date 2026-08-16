/**
 * The cycle — one pass of pull → three-way diff → resolve → push → commit.
 *
 * Implements the seven steps from App.md § 05 in their canonical order. The
 * ancestor update is always **last**: if any push fails, the ancestor stays
 * put and the next cycle retries cleanly. Advancing the ancestor before
 * pushes confirm is how silent drift creeps in over time.
 *
 * Guarantees:
 * - An empty changelog (no side moved) never writes anything.
 * - Unresolved conflicts block both the pushes *and* the ancestor update.
 * - Every push carries a deterministic idempotency key so retries collapse.
 * - Every push asserts the version the orchestrator last observed; mismatch
 *   raises `SyncError.staleWrite` inside the adapter and the cycle aborts.
 *
 * Shadow mode: `runShadow` performs the diff and resolution but skips the
 * push and ancestor update. Useful for running a new adapter alongside
 * production and reviewing the changelog it *would* have applied.
 */

import type { JsonValue } from "../domain/types.js";
import {
  type Changelog,
  threeWayDiff,
} from "../domain/diff/threeWay.js";
import { SyncError } from "../domain/error.js";
import { idempotencyKey } from "../domain/idempotency.js";
import { setAtPath } from "../domain/jsonPath.js";
import {
  AncestorEntry,
  AncestorKey,
  AncestorStoreError,
  type AncestorStore,
} from "../ports/ancestor.js";
import {
  EscalationError,
  EscalationItem,
  type EscalationQueue,
} from "../ports/escalation.js";
import type { ExternalRef, SystemPort } from "../ports/system.js";
import {
  MergeContext,
  PolicyMap,
  Resolution,
  resolve,
  type UnresolvedConflict,
} from "./policy/index.js";
import { InvariantSet } from "./policy/invariants.js";
import { OwnedBy } from "./policy/ownedBy.js";

/** Terminal result of one cycle. */
export type CycleOutcome =
  | { kind: "NoOp" }
  | { kind: "Synced"; pushedTo: string[] }
  | { kind: "Escalated"; conflicts: UnresolvedConflict[] };

/** Output of shadow mode — what the cycle *would* have done. */
export interface ShadowReport {
  changelog: Changelog;
  resolution: Resolution;
  /** Canonical value the cycle would have written, or `undefined` if escalated. */
  wouldWrite: JsonValue | undefined;
}

interface PreparedCycle {
  viewA: JsonValue;
  viewB: JsonValue;
  freshRefA: ExternalRef;
  freshRefB: ExternalRef;
  ancestorView: JsonValue;
  changelog: Changelog;
}

/**
 * Holds the two sides, the ancestor store, the policy map, and the
 * escalation queue. A single `Orchestrator` runs many cycles over time.
 */
export class Orchestrator {
  readonly sideA: SystemPort;
  readonly sideB: SystemPort;
  readonly ancestor: AncestorStore;
  readonly policies: PolicyMap;
  readonly escalation: EscalationQueue;
  /** Tier-2 post-merge invariants. Default empty; attach via `withInvariants`. */
  invariants: InvariantSet;

  constructor(
    sideA: SystemPort,
    sideB: SystemPort,
    ancestor: AncestorStore,
    policies: PolicyMap,
    escalation: EscalationQueue,
  ) {
    this.sideA = sideA;
    this.sideB = sideB;
    this.ancestor = ancestor;
    this.policies = policies;
    this.escalation = escalation;
    this.invariants = new InvariantSet();
  }

  /**
   * Attach a set of Tier-2 invariants. They run after Tier-1 resolution
   * produces a candidate merged value: a `Reject` blocks the cycle and
   * escalates; a `Transform` rewrites the value before push.
   */
  withInvariants(invariants: InvariantSet): this {
    this.invariants = invariants;
    return this;
  }

  /**
   * One-way orchestrator: `sideA` is the source of truth, `sideB` mirrors
   * it. Target-side edits revert on the next cycle (Synology's
   * "download only" semantics, expressed as an `OwnedBy` default).
   */
  static oneWay(
    sideA: SystemPort,
    sideB: SystemPort,
    ancestor: AncestorStore,
    escalation: EscalationQueue,
  ): Orchestrator {
    const policies = new PolicyMap().withDefault(new OwnedBy(sideA.systemType()));
    return new Orchestrator(sideA, sideB, ancestor, policies, escalation);
  }

  /**
   * Execute one full cycle. `nowMs` is the wall-clock timestamp stamped
   * onto the ancestor on success; tests pass a fixed value.
   */
  async runCycleAt(
    entityType: string,
    canonicalId: string,
    nowMs: number,
  ): Promise<CycleOutcome> {
    const prepared = await this.prepare(entityType, canonicalId);

    if (prepared.changelog.changes.length === 0) {
      return { kind: "NoOp" };
    }

    const ctx = new MergeContext(
      this.sideA.systemType(),
      this.sideB.systemType(),
    );
    const resolution = resolve(prepared.changelog, this.policies, ctx);

    if (!resolution.isClean()) {
      await this.enqueueEscalation(
        entityType,
        canonicalId,
        resolution.conflicts,
        nowMs,
      );
      return { kind: "Escalated", conflicts: resolution.conflicts };
    }

    const candidate = applyResolution(prepared.ancestorView, resolution);

    // Tier-2: post-merge invariants. A Transform rewrites the value; a
    // Reject blocks the cycle — the Tier-1 result was structurally valid
    // but violated a rule about entity state.
    const outcome = this.invariants.apply(prepared.ancestorView, candidate);
    let merged: JsonValue;
    switch (outcome.kind) {
      case "Pass":
        merged = candidate;
        break;
      case "Transform":
        merged = outcome.value;
        break;
      case "Reject": {
        const conflicts: UnresolvedConflict[] = [
          invariantConflict(outcome.reason, prepared.ancestorView, candidate),
        ];
        await this.enqueueEscalation(
          entityType,
          canonicalId,
          conflicts,
          nowMs,
        );
        return { kind: "Escalated", conflicts };
      }
      default: {
        const _exhaustive: never = outcome;
        throw new Error(
          `unreachable InvariantOutcome: ${JSON.stringify(_exhaustive)}`,
        );
      }
    }

    // Push stale sides. Deterministic A-then-B order keeps logs sane; a
    // mid-sequence failure leaves the ancestor untouched so the next cycle
    // re-derives everything.
    const pushedTo: string[] = [];
    if (!jsonEqual(merged, prepared.viewA)) {
      await this.pushTo(
        this.sideA,
        entityType,
        canonicalId,
        merged,
        prepared.freshRefA,
      );
      pushedTo.push(this.sideA.systemType());
    }
    if (!jsonEqual(merged, prepared.viewB)) {
      await this.pushTo(
        this.sideB,
        entityType,
        canonicalId,
        merged,
        prepared.freshRefB,
      );
      pushedTo.push(this.sideB.systemType());
    }

    // Commit the new ancestor LAST. Every earlier step retries
    // idempotently; this one must not happen until both sides confirmed.
    try {
      await this.ancestor.put(
        new AncestorKey(entityType, canonicalId),
        new AncestorEntry(merged, nowMs),
      );
    } catch (e) {
      throw SyncError.transient(describeError(e));
    }

    return { kind: "Synced", pushedTo };
  }

  /** Shadow mode — reports what a cycle would do without writing anywhere. */
  async runShadow(
    entityType: string,
    canonicalId: string,
  ): Promise<ShadowReport> {
    const prepared = await this.prepare(entityType, canonicalId);

    if (prepared.changelog.changes.length === 0) {
      return {
        changelog: prepared.changelog,
        resolution: new Resolution(),
        wouldWrite: undefined,
      };
    }

    const ctx = new MergeContext(
      this.sideA.systemType(),
      this.sideB.systemType(),
    );
    const resolution = resolve(prepared.changelog, this.policies, ctx);

    const wouldWrite = resolution.isClean()
      ? applyResolution(prepared.ancestorView, resolution)
      : undefined;

    return {
      changelog: prepared.changelog,
      resolution,
      wouldWrite,
    };
  }

  private async enqueueEscalation(
    entityType: string,
    canonicalId: string,
    conflicts: UnresolvedConflict[],
    nowMs: number,
  ): Promise<void> {
    try {
      await this.escalation.push(
        new EscalationItem(entityType, canonicalId, conflicts, nowMs),
      );
    } catch (e) {
      if (e instanceof EscalationError) {
        throw SyncError.transient(e.message);
      }
      throw SyncError.transient(describeError(e));
    }
  }

  private async pushTo(
    side: SystemPort,
    entityType: string,
    canonicalId: string,
    merged: JsonValue,
    freshRef: ExternalRef,
  ): Promise<void> {
    const ik = idempotencyKey(canonicalId, "upsert", merged);
    await side.upsert(
      entityType,
      canonicalId,
      merged,
      freshRef.version,
      ik,
    );
  }

  private async prepare(
    entityType: string,
    canonicalId: string,
  ): Promise<PreparedCycle> {
    const refA = await this.sideA.findByCanonicalId(entityType, canonicalId);
    if (refA === undefined) {
      throw SyncError.transient(
        `entity ${canonicalId} not found on ${this.sideA.systemType()}`,
      );
    }
    const refB = await this.sideB.findByCanonicalId(entityType, canonicalId);
    if (refB === undefined) {
      throw SyncError.transient(
        `entity ${canonicalId} not found on ${this.sideB.systemType()}`,
      );
    }

    const fetchedA = await this.sideA.fetch(entityType, refA);
    const fetchedB = await this.sideB.fetch(entityType, refB);
    const viewA = fetchedA.canonical;
    const viewB = fetchedB.canonical;

    // Bootstrap: missing ancestor treats A's current view as the baseline
    // so the first cycle propagates A→B.
    let ancestorView: JsonValue;
    try {
      const existing = await this.ancestor.get(
        new AncestorKey(entityType, canonicalId),
      );
      ancestorView = existing !== undefined ? existing.canonical : viewA;
    } catch (e) {
      if (e instanceof AncestorStoreError) {
        throw SyncError.transient(e.message);
      }
      throw SyncError.transient(describeError(e));
    }

    const changelog = threeWayDiff(ancestorView, viewA, viewB);

    return {
      viewA,
      viewB,
      freshRefA: fetchedA.ref,
      freshRefB: fetchedB.ref,
      ancestorView,
      changelog,
    };
  }
}

/** Wall-clock helper for tests that don't care about determinism. */
export function nowMs(): number {
  return Date.now();
}

/**
 * Start from the ancestor and overlay every `(path, value)` resolved by the
 * policies. Paths are dotted object keys (matching the three-way diff's
 * output). Intermediate objects are created when missing so resolutions onto
 * previously-unset nested fields work.
 */
export function applyResolution(
  base: JsonValue,
  resolution: Resolution,
): JsonValue {
  const out = deepClone(base);
  for (const [path, value] of resolution.resolved) {
    setAtPath(out, path, value);
  }
  return out;
}

/** Build the synthetic conflict that represents a Tier-2 invariant rejection. */
function invariantConflict(
  reason: string,
  ancestorView: JsonValue,
  candidate: JsonValue,
): UnresolvedConflict {
  return {
    path: "",
    reason,
    class: "InvariantViolation",
    change: {
      path: "",
      oldValue: ancestorView,
      newFromA: candidate,
      newFromB: undefined,
      source: "both",
    },
  };
}

function deepClone<T extends JsonValue>(v: T): T {
  // structuredClone is available in Node >=17. tsconfig targets ES2022 + Node
  // >=20 per package.json engines.
  return structuredClone(v);
}

function jsonEqual(a: JsonValue, b: JsonValue): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  if (typeof a !== typeof b) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!jsonEqual(a[i] as JsonValue, b[i] as JsonValue)) return false;
    }
    return true;
  }
  if (typeof a === "object" && typeof b === "object") {
    const aa = a as { [k: string]: JsonValue };
    const bb = b as { [k: string]: JsonValue };
    const aKeys = Object.keys(aa);
    const bKeys = Object.keys(bb);
    if (aKeys.length !== bKeys.length) return false;
    for (const k of aKeys) {
      if (!Object.prototype.hasOwnProperty.call(bb, k)) return false;
      if (!jsonEqual(aa[k] as JsonValue, bb[k] as JsonValue)) return false;
    }
    return true;
  }
  return false;
}

function describeError(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
