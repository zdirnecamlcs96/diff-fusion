/**
 * `Additive` — counters where both sides' deltas accumulate.
 *
 * For a field like `qtyReceived` where System A records 3 incoming units and
 * System B records 2 others during the same cycle, the merged value is
 * `ancestor + deltaA + deltaB = newA + newB - ancestor`.
 *
 * Only defined for numeric fields. Non-numeric inputs return a
 * `Conflict` rather than guessing.
 */

import { kernelMergeField } from "../../kernel.js";
import type { MergePolicyRef } from "./declaration.js";
import type {
  FieldChange,
  MergeContext,
  MergeOutcome,
  MergePolicy,
} from "./index.js";

export class Additive implements MergePolicy {
  name(): string {
    return "additive";
  }

  toRef(): MergePolicyRef {
    return { kind: "additive" };
  }

  merge(change: FieldChange, ctx: MergeContext): MergeOutcome {
    return kernelMergeField(change, this.toRef(), ctx);
  }
}
