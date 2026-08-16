import { describe, expect, it } from "vitest";
import { DiffFusion } from "../../../src/drivers/facade.js";
import type { JsonValue } from "../../../src/domain/types.js";

function testSchema(): JsonValue {
  return {
    cif_schema: {
      product_name: { type: "string", required: true },
      price: { type: "number", required: true },
    },
    transformations: {
      format_a: {
        product_name: { source_path: "name", type: "string" },
        price: { source_path: "price", type: "number" },
      },
    },
  };
}

// ---------------------------------------------------------------------------
// Ported Rust #[test] blocks (src/drivers/facade.rs)
// ---------------------------------------------------------------------------

describe("DiffFusion — ported Rust tests", () => {
  it("test_facade_creation", () => {
    const diffFusion = new DiffFusion(testSchema());
    // Rust: assert!(diff_fusion.schema().get("cif_schema").is_some())
    const schema = diffFusion.schema();
    expect(schema).not.toBeNull();
    expect(typeof schema).toBe("object");
    expect((schema as { cif_schema?: unknown }).cif_schema).toBeDefined();
  });

  it("test_facade_transform", () => {
    const diffFusion = new DiffFusion(testSchema());
    const source: JsonValue = { name: "Widget", price: 19.99 };

    const result = diffFusion.transform(source, "format_a");
    expect(result.ok).toBe(true);
    if (!result.ok) return;

    const cif = result.value as { [k: string]: JsonValue };
    expect(cif["product_name"]).toBe("Widget");
    expect(cif["price"]).toBe(19.99);
  });

  it("test_facade_compare", () => {
    const diffFusion = new DiffFusion(testSchema());
    const oldValue: JsonValue = { product_name: "Widget", price: 19.99 };
    const newValue: JsonValue = { product_name: "Widget", price: 24.99 };

    const report = diffFusion.compare(oldValue, newValue);
    expect(report.hasConflicts).toBe(true);
    expect(report.totalConflicts).toBe(1);
  });

  it("test_facade_transform_and_compare", () => {
    const diffFusion = new DiffFusion(testSchema());
    const sourceA: JsonValue = { name: "Widget", price: 19.99 };
    const sourceB: JsonValue = { name: "Widget", price: 24.99 };

    const result = diffFusion.transformAndCompare(
      sourceA,
      "format_a",
      sourceB,
      "format_a",
    );

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.report.hasConflicts).toBe(true);
  });

  it("test_validate_cif", () => {
    const diffFusion = new DiffFusion(testSchema());

    const valid: JsonValue = { product_name: "Widget", price: 19.99 };
    expect(diffFusion.validateCif(valid).ok).toBe(true);

    const missing: JsonValue = { product_name: "Widget" };
    const missingResult = diffFusion.validateCif(missing);
    expect(missingResult.ok).toBe(false);

    const wrongType: JsonValue = { product_name: "Widget", price: "not a number" };
    const wrongTypeResult = diffFusion.validateCif(wrongType);
    expect(wrongTypeResult.ok).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// TS-side behaviour: conflict contents, validation error paths
// ---------------------------------------------------------------------------

describe("DiffFusion — TS-side behaviour", () => {
  it("compare returns an empty report when values are identical", () => {
    const df = new DiffFusion(testSchema());
    const v: JsonValue = { product_name: "Widget", price: 19.99 };
    const r = df.compare(v, v);
    expect(r.hasConflicts).toBe(false);
    expect(r.totalConflicts).toBe(0);
    expect(r.conflicts).toEqual([]);
  });

  it("compare encodes leaf values as JSON-stringified strings on the conflict", () => {
    const df = new DiffFusion(testSchema());
    const r = df.compare({ price: 19.99 }, { price: 24.99 });
    expect(r.conflicts.length).toBe(1);
    expect(r.conflicts[0]!.path).toBe("price");
    expect(r.conflicts[0]!.oldValue).toBe("19.99");
    expect(r.conflicts[0]!.newValue).toBe("24.99");
  });

  it("compare walks nested objects and emits dotted paths", () => {
    const df = new DiffFusion(testSchema());
    const r = df.compare(
      { product: { name: "Widget", price: 19.99 } },
      { product: { name: "Widget", price: 24.99 } },
    );
    expect(r.totalConflicts).toBe(1);
    expect(r.conflicts[0]!.path).toBe("product.price");
  });

  it("transformAndCompare returns error when either side's transform fails", () => {
    const df = new DiffFusion(testSchema());
    const r = df.transformAndCompare(
      { name: "Widget", price: 19.99 },
      "format_missing",
      { name: "Widget", price: 19.99 },
      "format_a",
    );
    expect(r.ok).toBe(false);
  });

  it("validateCif reports a targeted message for missing required fields", () => {
    const df = new DiffFusion(testSchema());
    const r = df.validateCif({ product_name: "Widget" });
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.errors.some((e) => e.includes("Missing required field: price"))).toBe(
      true,
    );
  });

  it("validateCif reports type mismatches with the declared type", () => {
    const df = new DiffFusion(testSchema());
    const r = df.validateCif({ product_name: "Widget", price: "oops" });
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(
      r.errors.some((e) => /Field 'price': expected type 'number', got 'string'/.test(e)),
    ).toBe(true);
  });

  it("validateCif fails when schema lacks cif_schema", () => {
    const df = new DiffFusion({ transformations: {} });
    const r = df.validateCif({});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.errors).toEqual(["Schema missing 'cif_schema' definition"]);
  });

  it("validateCif fails when value is not an object", () => {
    const df = new DiffFusion(testSchema());
    const r = df.validateCif([] as JsonValue);
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.errors).toEqual(["CIF value must be an object"]);
  });
});

// ---------------------------------------------------------------------------
// Public re-exports from src/index.ts
// ---------------------------------------------------------------------------

describe("package entry point", () => {
  it("re-exports DiffFusion from the package root", async () => {
    const mod = await import("../../../src/index.js");
    expect(typeof mod.DiffFusion).toBe("function");
  });
});
