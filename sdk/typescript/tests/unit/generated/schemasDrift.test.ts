import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { CONFIGS, formatWithBiome, transformSchema } from "../../../scripts/gen-wire-schemas.js";

// Re-runs the generator's pure transform in-memory against the committed
// `spec/schema/*.json` and diffs the result against the committed
// `src/generated/*.ts` files. Catches "schema changed, forgot to regenerate"
// drift without needing cargo — the same golden-fixture pattern as
// `spec/vectors/`.

const TS_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const SCHEMA_DIR = path.resolve(TS_ROOT, "../../spec/schema");
const OUT_DIR = path.resolve(TS_ROOT, "src/generated");

describe("generated wire schemas match spec/schema/*.json", () => {
  for (const config of CONFIGS) {
    it(`${config.outFile} is up to date with ${config.schemaFile}`, () => {
      const schemaJson = JSON.parse(readFileSync(path.join(SCHEMA_DIR, config.schemaFile), "utf8"));
      const raw = transformSchema(config, schemaJson);
      const expected = formatWithBiome(raw, config.outFile);
      const actual = readFileSync(path.join(OUT_DIR, config.outFile), "utf8");
      expect(actual).toBe(expected);
    });
  }
});
