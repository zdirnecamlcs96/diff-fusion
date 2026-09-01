import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  mergePolicyRefSchema,
  policyDocumentSchema,
} from "../../src/application/policy/declaration.js";
import { mergeOutcomeSchema } from "../../src/application/policy/index.js";
import {
  batchResolutionSchema,
  fuseResultSchema,
  kernelCompareJsonRaw,
  kernelFuseRaw,
  kernelMergeBatchRaw,
  kernelMergeFieldRaw,
  kernelThreeWayDiffRaw,
  kernelTransformToCifRaw,
  wireChangelogSchema,
  wireFieldChangeSchema,
} from "../../src/kernel.js";

// Every field below is a JSON-encoded string, not a parsed value: the vector
// file hands each runtime byte-identical input so nothing gets reshaped by
// JSON.stringify/serde/encoding-json on the way in.
type ThreeWayDiffVector = {
  name: string;
  ancestor: string;
  a: string;
  b: string;
  expected: string;
  isErr: boolean;
};

type MergeFieldVector = {
  name: string;
  change: string;
  policyRef: string;
  ctx: string;
  expected: string;
  isErr: boolean;
};

type MergeBatchVector = {
  name: string;
  changelog: string;
  policyDoc: string;
  ctx: string;
  expected: string;
  isErr: boolean;
};

type FuseVector = {
  name: string;
  ancestor: string;
  a: string;
  b: string;
  policyDoc: string;
  ctx: string;
  expected: string;
  isErr: boolean;
};

type CompareJsonVector = {
  name: string;
  a: string;
  b: string;
  expected: string;
  isErr: boolean;
};

type TransformToCifVector = {
  name: string;
  source: string;
  schema: string;
  formatId: string;
  expected: string;
  isErr: boolean;
};

type KernelVectors = {
  threeWayDiff: ThreeWayDiffVector[];
  mergeField: MergeFieldVector[];
  mergeBatch: MergeBatchVector[];
  fuse: FuseVector[];
  compareJson: CompareJsonVector[];
  transformToCif: TransformToCifVector[];
};

const fixtureUrl = new URL(
  "../../../../spec/vectors/kernel-vectors.json",
  import.meta.url,
);
const vectors = JSON.parse(
  readFileSync(fileURLToPath(fixtureUrl), "utf8"),
) as KernelVectors;

const EXPECTED_THREE_WAY_DIFF = 17;
const EXPECTED_MERGE_FIELD = 29;
const EXPECTED_MERGE_BATCH = 5;
const EXPECTED_FUSE = 7;
const EXPECTED_COMPARE_JSON = 8;
const EXPECTED_TRANSFORM_TO_CIF = 13;
if (
  vectors.threeWayDiff.length !== EXPECTED_THREE_WAY_DIFF ||
  vectors.mergeField.length !== EXPECTED_MERGE_FIELD ||
  vectors.mergeBatch.length !== EXPECTED_MERGE_BATCH ||
  vectors.fuse.length !== EXPECTED_FUSE ||
  vectors.compareJson.length !== EXPECTED_COMPARE_JSON ||
  vectors.transformToCif.length !== EXPECTED_TRANSFORM_TO_CIF
) {
  throw new Error(
    `expected ${EXPECTED_THREE_WAY_DIFF} threeWayDiff + ${EXPECTED_MERGE_FIELD} mergeField + ${EXPECTED_MERGE_BATCH} mergeBatch + ${EXPECTED_FUSE} fuse + ${EXPECTED_COMPARE_JSON} compareJson + ${EXPECTED_TRANSFORM_TO_CIF} transformToCif vectors, got ${vectors.threeWayDiff.length} + ${vectors.mergeField.length} + ${vectors.mergeBatch.length} + ${vectors.fuse.length} + ${vectors.compareJson.length} + ${vectors.transformToCif.length}`,
  );
}

/** Runs `fn`, returns the thrown error's message, or fails if it didn't throw. */
function messageOf(fn: () => void): string {
  try {
    fn();
  } catch (e) {
    expect(e).toBeInstanceOf(Error);
    return (e as Error).message;
  }
  throw new Error("expected function to throw");
}

describe("kernel vector conformance", () => {
  describe("three_way_diff", () => {
    for (const v of vectors.threeWayDiff) {
      it(v.name, () => {
        if (v.isErr) {
          expect(messageOf(() => kernelThreeWayDiffRaw(v.ancestor, v.a, v.b))).toBe(
            v.expected,
          );
        } else {
          expect(kernelThreeWayDiffRaw(v.ancestor, v.a, v.b)).toBe(v.expected);
        }
      });
    }
  });

  describe("merge_field", () => {
    for (const v of vectors.mergeField) {
      it(v.name, () => {
        if (v.isErr) {
          expect(
            messageOf(() => kernelMergeFieldRaw(v.change, v.policyRef, v.ctx)),
          ).toBe(v.expected);
        } else {
          expect(kernelMergeFieldRaw(v.change, v.policyRef, v.ctx)).toBe(
            v.expected,
          );
        }
      });
    }
  });

  describe("merge_batch", () => {
    for (const v of vectors.mergeBatch) {
      it(v.name, () => {
        if (v.isErr) {
          expect(
            messageOf(() => kernelMergeBatchRaw(v.changelog, v.policyDoc, v.ctx)),
          ).toBe(v.expected);
        } else {
          expect(kernelMergeBatchRaw(v.changelog, v.policyDoc, v.ctx)).toBe(
            v.expected,
          );
        }
      });
    }
  });

  describe("fuse", () => {
    for (const v of vectors.fuse) {
      it(v.name, () => {
        if (v.isErr) {
          expect(
            messageOf(() => kernelFuseRaw(v.ancestor, v.a, v.b, v.policyDoc, v.ctx)),
          ).toBe(v.expected);
        } else {
          expect(kernelFuseRaw(v.ancestor, v.a, v.b, v.policyDoc, v.ctx)).toBe(
            v.expected,
          );
        }
      });
    }
  });

  describe("compare_json", () => {
    for (const v of vectors.compareJson) {
      it(v.name, () => {
        if (v.isErr) {
          expect(messageOf(() => kernelCompareJsonRaw(v.a, v.b))).toBe(v.expected);
        } else {
          expect(kernelCompareJsonRaw(v.a, v.b)).toBe(v.expected);
        }
      });
    }
  });

  describe("transform_to_cif", () => {
    for (const v of vectors.transformToCif) {
      it(v.name, () => {
        if (v.isErr) {
          expect(
            messageOf(() => kernelTransformToCifRaw(v.source, v.schema, v.formatId)),
          ).toBe(v.expected);
        } else {
          expect(kernelTransformToCifRaw(v.source, v.schema, v.formatId)).toBe(
            v.expected,
          );
        }
      });
    }
  });
});

describe("zod schema drift guard", () => {
  for (const v of vectors.threeWayDiff.filter((v) => !v.isErr)) {
    it(`${v.name}: wireChangelogSchema accepts kernel output`, () => {
      expect(() => wireChangelogSchema.parse(JSON.parse(v.expected))).not.toThrow();
    });
  }
  for (const v of vectors.mergeField.filter((v) => !v.isErr)) {
    it(`${v.name}: schemas accept vector payloads`, () => {
      expect(() => wireFieldChangeSchema.parse(JSON.parse(v.change))).not.toThrow();
      expect(() => mergePolicyRefSchema.parse(JSON.parse(v.policyRef))).not.toThrow();
      expect(() => mergeOutcomeSchema.parse(JSON.parse(v.expected))).not.toThrow();
    });
  }
  for (const v of vectors.mergeBatch.filter((v) => !v.isErr)) {
    it(`${v.name}: schemas accept vector payloads`, () => {
      expect(() => wireChangelogSchema.parse(JSON.parse(v.changelog))).not.toThrow();
      expect(() => policyDocumentSchema.parse(JSON.parse(v.policyDoc))).not.toThrow();
      expect(() => batchResolutionSchema.parse(JSON.parse(v.expected))).not.toThrow();
    });
  }
  for (const v of vectors.fuse.filter((v) => !v.isErr)) {
    it(`${v.name}: schemas accept vector payloads`, () => {
      expect(() => policyDocumentSchema.parse(JSON.parse(v.policyDoc))).not.toThrow();
      expect(() => fuseResultSchema.parse(JSON.parse(v.expected))).not.toThrow();
    });
  }
});
