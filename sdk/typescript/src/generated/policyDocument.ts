// Code generated from spec/schema/policy-document.schema.json by sdk/typescript/scripts/gen-wire-schemas.ts; DO NOT EDIT.

import { z } from "zod";
import { mergePolicyRefSchema, type MergePolicyRef } from "./policyConfig.js";

export const policyDocumentSchema = z.object({
  default: mergePolicyRefSchema
    .nullish()
    .transform((v) => v ?? undefined)
    .optional(),
  fields: z.record(z.string(), mergePolicyRefSchema).default({}),
});
export type PolicyDocument = z.infer<typeof policyDocumentSchema>;
