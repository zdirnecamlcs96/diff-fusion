// Code generated from spec/schema/merge-outcome.schema.json by sdk/typescript/scripts/gen-wire-schemas.ts; DO NOT EDIT.

import { z } from "zod";
import { jsonValueSchema } from "../domain/types.js";

export const mergeOutcomeSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("Resolved"),
    value: jsonValueSchema,
  }),
  z.object({
    kind: z.literal("Conflict"),
    reason: z.string(),
  }),
]);
export type MergeOutcome = z.infer<typeof mergeOutcomeSchema>;
