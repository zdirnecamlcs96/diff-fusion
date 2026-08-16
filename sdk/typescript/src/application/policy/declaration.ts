/**
 * Schema-side declarations for merge policies.
 *
 * {@link MergePolicyRef} is the JSON-serialisable shape that lives in the
 * schema document. At runtime the orchestrator calls {@link build} to turn
 * each declaration into a {@link MergePolicy} instance.
 *
 * `LastWriteWins` is declarable here, but every declaration still carries
 * the same written `reason` and per-cycle timestamps the runtime
 * constructor requires — see `escapeHatch.ts` for why it should almost
 * never be a schema's default strategy.
 *
 * `SetByKey`'s full per-side add/remove/union configuration (including
 * recursive `nested` policies) round-trips through the wire via
 * {@link SetByKey.toRef}, and `build()` applies every field back onto the
 * instantiated `SetByKey` — see {@link SetByKey.merge}, which delegates to
 * the kernel through the same ref.
 *
 * Wire format matches Rust `#[serde(tag = "kind", rename_all = "snake_case")]`
 * byte-for-byte so schema JSON documents round-trip across runtimes.
 */

import { Additive } from "./additive.js";
import { Append } from "./append.js";
import { LastWriteWins } from "./escapeHatch.js";
import { type MergePolicy, PolicyMap } from "./index.js";
import { OwnedBy } from "./ownedBy.js";
import { StateMachine, StateTransition } from "./stateMachine.js";
import { SetByKey } from "./structural.js";
import {
  mergePolicyRefSchema,
  transitionRefSchema,
  type MergePolicyRef,
  type SetByKeyRef,
  type TransitionRef,
} from "../../generated/policyConfig.js";
import {
  policyDocumentSchema,
  type PolicyDocument,
} from "../../generated/policyDocument.js";

/** Flat wire shape for a single `(from, to)` allowed state transition. */
export { transitionRefSchema, type TransitionRef };

/**
 * Serialisable declaration of a per-field merge policy.
 *
 * The literal `kind` tag is the serde discriminator, and the variant names
 * (`"owned_by"`, `"additive"`, `"append"`, `"state_machine"`, `"set_by_key"`)
 * match Rust's `rename_all = "snake_case"` output exactly. `set_by_key`
 * carries the full per-side add/remove/union configuration from
 * `structural.ts`'s `SetByKey` — see `SetByKey.toRef()`.
 */
export { mergePolicyRefSchema, type MergePolicyRef };

/** Instantiate the runtime {@link MergePolicy} for a declaration. */
export function build(ref: MergePolicyRef): MergePolicy {
  switch (ref.kind) {
    case "owned_by":
      return new OwnedBy(ref.system);
    case "additive":
      return new Additive();
    case "append":
      return new Append();
    case "state_machine":
      return new StateMachine(
        ref.transitions.map((t) => new StateTransition(t.from, t.to)),
      );
    case "last_write_wins":
      return new LastWriteWins(ref.reason, ref.timestamp_a, ref.timestamp_b);
    case "set_by_key":
      return buildSetByKey(ref);
    default: {
      const _exhaustive: never = ref;
      throw new Error(
        `unreachable MergePolicyRef: ${JSON.stringify(_exhaustive)}`,
      );
    }
  }
}

/** Instantiate a {@link SetByKey}, applying every field from its wire ref
 * (including recursive `nested` policies). Mirrors Rust's
 * `From<&SetByKeyRef> for SetByKey`. */
function buildSetByKey(ref: SetByKeyRef): SetByKey {
  const policy = new SetByKey(ref.identity, ref.a_anchor, ref.b_anchor);
  policy.onAddedInA = ref.on_added_in_a;
  policy.onAddedInB = ref.on_added_in_b;
  policy.onRemovedInA = ref.on_removed_in_a;
  policy.onRemovedInB = ref.on_removed_in_b;
  policy.onBothChanged = ref.on_both_changed;
  policy.preferAOnFieldConflict = ref.prefer_a_on_field_conflict;
  for (const [field, nestedRef] of Object.entries(ref.nested)) {
    policy.nested.set(field, buildSetByKey(nestedRef));
  }
  return policy;
}

/**
 * A whole entity type's worth of field policies, as authored by a host
 * (e.g. a JSON document in a `jsonb` column). The entity type is the lookup
 * key into a policy store, not embedded here — mirrors `AncestorKey` being
 * separate from the entry it addresses.
 *
 * Wire format matches Rust `PolicyDocument` byte-for-byte: `fields` defaults
 * to `{}` when absent, and `default` is omitted from JSON entirely when
 * unset (never serialised as `"default": null`).
 */
export { policyDocumentSchema, type PolicyDocument };

/**
 * Parse a {@link PolicyDocument} from raw JSON, applying the same defaults
 * as Rust's `#[serde(default)]`: an absent `fields` key becomes `{}`, and an
 * absent (or `null`) `default` key means no default policy. Throws on any
 * other shape.
 */
export function parsePolicyDocument(value: unknown): PolicyDocument {
  // ponytail: raw ZodError surfaces to callers; wrap into a typed error if a
  // caller ever needs to discriminate parse failures from other Errors — none does.
  return policyDocumentSchema.parse(value);
}

/** Instantiate a {@link PolicyMap} from a document's declarations. */
export function buildPolicyDocument(doc: PolicyDocument): PolicyMap {
  const map = new PolicyMap();
  for (const [path, decl] of Object.entries(doc.fields)) {
    map.with(path, build(decl));
  }
  if (doc.default !== undefined) {
    map.withDefault(build(doc.default));
  }
  return map;
}
