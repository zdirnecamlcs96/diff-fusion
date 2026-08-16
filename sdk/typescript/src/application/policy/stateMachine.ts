/**
 * `StateMachine` — enum fields with declared allowed transitions.
 *
 * For a field like `status` with values `draft | open | closed | cancelled`,
 * the policy rejects transitions not listed in its `allowed` set. This
 * prevents corrupt merges like "closed -> draft" from silently taking effect.
 *
 * When both sides move along different branches, the policy escalates —
 * picking a winner would require business judgement the policy doesn't have.
 */

import { kernelMergeField } from "../../kernel.js";
import type { MergePolicyRef } from "./declaration.js";
import type {
  FieldChange,
  MergeContext,
  MergeOutcome,
  MergePolicy,
} from "./index.js";

/** One allowed `from -> to` transition. */
export class StateTransition {
  readonly from: string;
  readonly to: string;

  constructor(from: string, to: string) {
    this.from = from;
    this.to = to;
  }
}

/**
 * A state machine with a set of allowed transitions.
 *
 * All string-valued states that do not appear in any transition are treated
 * as unreachable — a move *to* an unknown state is rejected.
 */
export class StateMachine implements MergePolicy {
  private readonly allowed: readonly StateTransition[];

  constructor(allowed: Iterable<StateTransition>) {
    this.allowed = [...allowed];
  }

  name(): string {
    return "state_machine";
  }

  toRef(): MergePolicyRef {
    return {
      kind: "state_machine",
      transitions: this.allowed.map((t) => ({ from: t.from, to: t.to })),
    };
  }

  merge(change: FieldChange, ctx: MergeContext): MergeOutcome {
    return kernelMergeField(change, this.toRef(), ctx);
  }
}
