import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import * as kernel from "../../src/kernel.js";

// Deliberate exception to "kernel.ts is the only file allowed to touch the
// wasm module": this test's whole job is to catch drift between the wasm
// glue's actual exports and what kernel.ts wraps, so it needs the raw module.
const require = createRequire(import.meta.url);
const wasm = require("../../wasm/diff_fusion.js");

/**
 * Every function the Rust wasm kernel exports, hand-copied from the
 * `export!` macro invocations in `core/src/drivers/wasm.rs` and
 * cross-checked against `sdk/typescript/wasm/diff_fusion.js`.
 *
 * New kernel export? Add its wrapper in kernel.ts, then add it here.
 * Hardcoded on purpose — see kernelVectors.test.ts.
 */
const EXPECTED_WASM_EXPORTS = [
  "canonical_json",
  "compare_json",
  "fuse",
  "idempotency_key_hex",
  "merge_batch",
  "merge_field",
  "three_way_diff",
  "transform_to_cif",
] as const;

// wasm-bindgen glue also exports internal plumbing (__wbindgen_*, __wasm) —
// not part of the kernel API surface, excluded here.
function wasmKernelExportNames(): string[] {
  return Object.keys(wasm)
    .filter((name) => typeof (wasm as Record<string, unknown>)[name] === "function")
    .filter((name) => !name.startsWith("_"))
    .sort();
}

describe("kernel wasm export parity", () => {
  it("wasm module exports exactly the expected kernel functions", () => {
    expect(wasmKernelExportNames()).toEqual([...EXPECTED_WASM_EXPORTS].sort());
  });

  it("kernel.ts wraps every expected wasm export", () => {
    const wrapperFor: Record<(typeof EXPECTED_WASM_EXPORTS)[number], unknown> = {
      canonical_json: kernel.kernelCanonicalJson,
      compare_json: kernel.kernelCompareJson,
      fuse: kernel.kernelFuse,
      idempotency_key_hex: kernel.kernelIdempotencyKeyHex,
      merge_batch: kernel.kernelMergeBatch,
      merge_field: kernel.kernelMergeField,
      three_way_diff: kernel.kernelThreeWayDiff,
      transform_to_cif: kernel.kernelTransformToCif,
    };
    for (const name of EXPECTED_WASM_EXPORTS) {
      expect(typeof wrapperFor[name]).toBe("function");
    }
  });
});
