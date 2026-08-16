/**
 * Shared test fixtures — port of `tests/common/helpers.rs`.
 *
 * These are schema JSON documents used across multiple test suites.
 * Keep the shape byte-identical to the Rust originals so cross-language
 * test-vector comparisons stay trivial.
 */

import type { JsonValue } from "../src/domain/types.js";

/** Basic product schema for simple transform/diff tests. */
export function basicProductSchema(): JsonValue {
  return {
    cif_schema: {
      product_name: { type: "string", required: true },
      product_price: { type: "number", required: true },
    },
    transformations: {
      format_a: {
        product_name: { source_path: "name", type: "string" },
        product_price: { source_path: "price", type: "number" },
      },
    },
  };
}

/** Schema exercising dotted source_path traversal. */
export function nestedSchema(): JsonValue {
  return {
    cif_schema: {
      product_name: { type: "string", required: true },
      product_price: { type: "number", required: true },
    },
    transformations: {
      nested_format: {
        product_name: {
          source_path: "product.details.name",
          type: "string",
        },
        product_price: { source_path: "pricing.amount", type: "number" },
      },
    },
  };
}

/** Schema exercising string → number / boolean coercion. */
export function typeConversionSchema(): JsonValue {
  return {
    cif_schema: {
      product_id: { type: "string", required: true },
      is_active: { type: "boolean", required: true },
      stock: { type: "number", required: true },
    },
    transformations: {
      format_b: {
        product_id: { source_path: "id", type: "string" },
        is_active: { source_path: "active", type: "boolean" },
        stock: { source_path: "quantity", type: "number" },
      },
    },
  };
}

/** Multi-system schema used by end-to-end sync tests (System A / System B). */
export function multiSystemSchema(): JsonValue {
  return {
    cif_schema: {
      product_id: { type: "string", required: true },
      quantity: { type: "number", required: true },
    },
    transformations: {
      system_a: {
        product_id: { source_path: "id", type: "string" },
        quantity: { source_path: "stock", type: "number" },
      },
      system_b: {
        product_id: { source_path: "product_id", type: "string" },
        quantity: { source_path: "inventory", type: "number" },
      },
    },
  };
}
