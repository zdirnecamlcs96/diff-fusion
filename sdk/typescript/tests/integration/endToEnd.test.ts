/**
 * End-to-end: transform two systems' raw shapes into CIF, then compare.
 *
 * Port of `tests/integration/end_to_end_tests.rs`. This lives at the Tier-0
 * seam — it uses only transform + compare, no SyncEngine. Kept here rather
 * than in unit tests because it stitches multiple primitives together.
 */

import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../src/domain/types.js";
import { compareJson } from "../../src/domain/compare.js";
import { toCif } from "../../src/application/transform.js";
import { multiSystemSchema } from "../helpers.js";

describe("test_end_to_end_transform_and_compare", () => {
  it("two system shapes converge through CIF, diff reveals only quantity", () => {
    const systemA: JsonValue = { id: "P123", stock: 100 };
    const systemB: JsonValue = { product_id: "P123", inventory: 95 };
    const schema = multiSystemSchema();

    const cifA = toCif(systemA, schema, "system_a");
    const cifB = toCif(systemB, schema, "system_b");
    expect(cifA.ok).toBe(true);
    expect(cifB.ok).toBe(true);
    if (!cifA.ok || !cifB.ok) return;

    const diffs = compareJson(cifA.value, cifB.value);
    expect(diffs).toHaveLength(1);
    const [path, [oldVal, newVal]] = diffs[0]!;
    expect(path).toBe("quantity");
    expect(oldVal).toBe(100);
    expect(newVal).toBe(95);
  });
});
