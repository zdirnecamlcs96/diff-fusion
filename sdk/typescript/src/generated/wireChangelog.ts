// Code generated from spec/schema/wire-changelog.schema.json by sdk/typescript/scripts/gen-wire-schemas.ts; DO NOT EDIT.

import { z } from "zod";
import { jsonValueSchema } from "../domain/types.js";

export const changeSourceSchema = z.enum(["a", "b", "both"]);
export type ChangeSource = z.infer<typeof changeSourceSchema>;

export const wireFieldChangeSchema = z.object({
  new_from_a: jsonValueSchema.optional(),
  new_from_b: jsonValueSchema.optional(),
  old_value: jsonValueSchema,
  path: z.string(),
  source: changeSourceSchema,
});
export type WireFieldChange = z.infer<typeof wireFieldChangeSchema>;

export const wireChangelogSchema = z.object({
  changes: z.array(wireFieldChangeSchema),
});
export type WireChangelog = z.infer<typeof wireChangelogSchema>;
