/**
 * `Append` — concatenate both sides' additions.
 *
 * For array fields like `notes` or `tags`, the merged value is the union of
 * A's and B's additions to the ancestor, preserving order (A's additions
 * before B's). Duplicates introduced by independent additions are kept by
 * default; deduplication is a separate concern (callers can post-process).
 *
 * Non-array fields return a `Conflict`. String concatenation is
 * intentionally not supported — git-style merges on free-text business
 * prose produce garbage.
 */

import { kernelMergeField } from "../../kernel.js";
import type { MergePolicyRef } from "./declaration.js";
import type {
  FieldChange,
  MergeContext,
  MergeOutcome,
  MergePolicy,
} from "./index.js";

export class Append implements MergePolicy {
  name(): string {
    return "append";
  }

  toRef(): MergePolicyRef {
    return { kind: "append" };
  }

  merge(change: FieldChange, ctx: MergeContext): MergeOutcome {
    return kernelMergeField(change, this.toRef(), ctx);
  }
}
