/**
 * `OwnedBy` — one system is authoritative for the field.
 *
 * The owner's changes propagate; the non-owner's changes to this field are
 * ignored (the owner's last known value wins). The single most effective
 * strategy — eliminates ~80% of conflicts before they arise.
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
 * Owner is declared by system label — must match `ctx.system_a` or
 * `ctx.system_b` at merge time.
 */
export class OwnedBy implements MergePolicy {
  readonly system: string;

  constructor(system: string) {
    this.system = system;
  }

  name(): string {
    return "owned_by";
  }

  toRef(): MergePolicyRef {
    return { kind: "owned_by", system: this.system };
  }

  merge(change: FieldChange, ctx: MergeContext): MergeOutcome {
    return kernelMergeField(change, this.toRef(), ctx);
  }
}
