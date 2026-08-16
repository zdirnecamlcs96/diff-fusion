/**
 * Escape hatches — policies that exist but should almost never be your first
 * choice.
 *
 * {@link LastWriteWins} lives here instead of alongside `OwnedBy`/`Additive`
 * on purpose. Timestamp-based conflict resolution fails at scale: clock skew
 * between systems, batch-ingest windows, and queue backpressure all make
 * "most recent write" an unreliable signal. Using it implicitly produces the
 * shopping-cart-bug class of regression that motivated CRDT research.
 *
 * A `LastWriteWins` cannot be constructed without a written justification.
 * The `reason` field is displayed in logs and conflict reports so reviewers
 * can see why the escape hatch was taken for a given field.
 */

import { kernelMergeField } from "../../kernel.js";
import type { MergePolicyRef } from "./declaration.js";
import type {
  FieldChange,
  MergeContext,
  MergeOutcome,
  MergePolicy,
} from "./index.js";

/**
 * Pick the side with the most recent write.
 *
 * Requires per-field timestamp metadata — not always available from external
 * systems, so the caller supplies it via `timestampAMs` / `timestampBMs`
 * (millisecond epochs; the larger value wins). Ties escalate.
 *
 * The `reason` argument is mandatory and surfaces in logs/conflict reports.
 */
export class LastWriteWins implements MergePolicy {
  readonly reason: string;
  readonly timestampA: number;
  readonly timestampB: number;

  /**
   * Construct with an explicit justification. There is no default constructor
   * on purpose — every use site must spell out why. `reason` is a required
   * positional argument; passing an empty string is a type-level no-op but a
   * review-time smell.
   */
  constructor(reason: string, timestampAMs: number, timestampBMs: number) {
    this.reason = reason;
    this.timestampA = timestampAMs;
    this.timestampB = timestampBMs;
  }

  name(): string {
    return "last_write_wins";
  }

  toRef(): MergePolicyRef {
    return {
      kind: "last_write_wins",
      reason: this.reason,
      timestamp_a: this.timestampA,
      timestamp_b: this.timestampB,
    };
  }

  merge(change: FieldChange, ctx: MergeContext): MergeOutcome {
    return kernelMergeField(change, this.toRef(), ctx);
  }
}
