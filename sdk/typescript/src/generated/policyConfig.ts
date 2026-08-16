// Code generated from spec/schema/policy-config.schema.json by sdk/typescript/scripts/gen-wire-schemas.ts; DO NOT EDIT.

import { z } from "zod";

export const transitionRefSchema = z.object({
  from: z.string(),
  to: z.string(),
});
export type TransitionRef = z.infer<typeof transitionRefSchema>;

export const onAddedSchema = z.enum(["Include", "Exclude"]);
export type OnAdded = z.infer<typeof onAddedSchema>;

export const onBothChangedSchema = z.enum(["Escalate", "PreferA", "PreferB", "Union"]);
export type OnBothChanged = z.infer<typeof onBothChangedSchema>;

export const onRemovedSchema = z.enum(["Remove", "EscalateIfChanged"]);
export type OnRemoved = z.infer<typeof onRemovedSchema>;

export interface SetByKeyRef {
  a_anchor: string;
  b_anchor: string;
  identity: string[];
  nested: Record<string, SetByKeyRef>;
  on_added_in_a: OnAdded;
  on_added_in_b: OnAdded;
  on_both_changed: OnBothChanged;
  on_removed_in_a: OnRemoved;
  on_removed_in_b: OnRemoved;
  prefer_a_on_field_conflict: boolean;
}

export const setByKeyRefSchema: z.ZodType<SetByKeyRef> = z.lazy(() =>
  z.object({
    a_anchor: z.string(),
    b_anchor: z.string(),
    identity: z.array(z.string()),
    nested: z.record(z.string(), setByKeyRefSchema).default({}),
    on_added_in_a: onAddedSchema.default("Include"),
    on_added_in_b: onAddedSchema.default("Include"),
    on_both_changed: onBothChangedSchema.default("Escalate"),
    on_removed_in_a: onRemovedSchema.default("EscalateIfChanged"),
    on_removed_in_b: onRemovedSchema.default("EscalateIfChanged"),
    prefer_a_on_field_conflict: z.boolean().default(true),
  }),
);

export const mergePolicyRefSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("owned_by"),
    system: z.string(),
  }),
  z.object({
    kind: z.literal("additive"),
  }),
  z.object({
    kind: z.literal("append"),
  }),
  z.object({
    kind: z.literal("state_machine"),
    transitions: z.array(transitionRefSchema),
  }),
  z.object({
    kind: z.literal("last_write_wins"),
    reason: z.string(),
    timestamp_a: z.number().int().nonnegative(),
    timestamp_b: z.number().int().nonnegative(),
  }),
  z.object({
    a_anchor: z.string(),
    b_anchor: z.string(),
    identity: z.array(z.string()),
    kind: z.literal("set_by_key"),
    nested: z.record(z.string(), setByKeyRefSchema).default({}),
    on_added_in_a: onAddedSchema.default("Include"),
    on_added_in_b: onAddedSchema.default("Include"),
    on_both_changed: onBothChangedSchema.default("Escalate"),
    on_removed_in_a: onRemovedSchema.default("EscalateIfChanged"),
    on_removed_in_b: onRemovedSchema.default("EscalateIfChanged"),
    prefer_a_on_field_conflict: z.boolean().default(true),
  }),
]);
export type MergePolicyRef = z.infer<typeof mergePolicyRefSchema>;
