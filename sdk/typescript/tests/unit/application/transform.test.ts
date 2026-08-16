import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../../src/domain/types.js";
import {
  toCif,
  toCifString,
  transformToCif,
} from "../../../src/application/transform.js";
import {
  basicProductSchema,
  nestedSchema,
  typeConversionSchema,
} from "../../helpers.js";

describe("test_transform_basic", () => {
  it("maps simple fields by source_path", () => {
    const source: JsonValue = { name: "Widget", price: 19.99 };
    const r = toCif(source, basicProductSchema(), "format_a");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ product_name: "Widget", product_price: 19.99 });
    }
  });
});

describe("test_transform_nested_path", () => {
  it("resolves dotted source_path through nested objects", () => {
    const source: JsonValue = {
      product: { details: { name: "Gadget" } },
      pricing: { amount: 99.99 },
    };
    const r = toCif(source, nestedSchema(), "nested_format");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({
        product_name: "Gadget",
        product_price: 99.99,
      });
    }
  });
});

describe("test_transform_type_conversion", () => {
  it("coerces strings to number/boolean per target type", () => {
    const source: JsonValue = {
      id: "12345",
      active: "true",
      quantity: "100",
    };
    const r = toCif(source, typeConversionSchema(), "format_b");
    expect(r.ok).toBe(true);
    if (r.ok) {
      const v = r.value as { [k: string]: JsonValue };
      expect(v.product_id).toBe("12345");
      expect(v.is_active).toBe(true);
      expect(v.stock).toBe(100);
    }
  });
});

describe("test_transform_missing_required_field", () => {
  it("errors when a required field's source_path is absent", () => {
    const source: JsonValue = { name: "Widget" };
    const r = toCif(source, basicProductSchema(), "format_a");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("Required field 'product_price'");
    }
  });
});

describe("test_transform_array_of_objects_with_element_schema", () => {
  it("maps line items with anchor-carrying element schema", () => {
    const schema: JsonValue = {
      cif_schema: {
        items: {
          type: "array",
          required: true,
          element: {
            externalId: { type: "string", anchor: "a" },
            internalId: { type: "string", anchor: "b" },
            sku: { type: "string", required: true },
            qty: { type: "number" },
          },
        },
      },
      transformations: {
        erp: {
          items: {
            source_path: "lineItems",
            type: "array",
            element: {
              externalId: { source_path: "extId", type: "string" },
              sku: { source_path: "sku", type: "string" },
              qty: { source_path: "quantity", type: "number" },
            },
          },
        },
      },
    };
    const source: JsonValue = {
      lineItems: [
        { extId: "A-1", sku: "SKU-X", quantity: 3 },
        { extId: "A-2", sku: "SKU-Y", quantity: 5 },
      ],
    };
    const r = toCif(source, schema, "erp");
    expect(r.ok).toBe(true);
    if (r.ok) {
      const value = r.value as { items: Array<{ [k: string]: JsonValue }> };
      expect(value.items).toHaveLength(2);
      expect(value.items[0]?.externalId).toBe("A-1");
      expect(value.items[0]?.sku).toBe("SKU-X");
      expect(value.items[0]?.qty).toBe(3);
      // internalId is declared on the CIF element but absent from the
      // transformation — should be absent from output.
      expect(value.items[0]).not.toHaveProperty("internalId");
    }
  });
});

describe("test_transform_array_element_required_field_missing_errors", () => {
  it("errors when a required element field is missing", () => {
    const schema: JsonValue = {
      cif_schema: {
        items: {
          type: "array",
          element: {
            sku: { type: "string", required: true },
          },
        },
      },
      transformations: {
        erp: {
          items: {
            source_path: "lineItems",
            type: "array",
            element: {
              sku: { source_path: "sku", type: "string" },
            },
          },
        },
      },
    };
    const source: JsonValue = {
      lineItems: [{ sku: "SKU-X" }, {}],
    };
    const r = toCif(source, schema, "erp");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("required element field 'items.sku'");
    }
  });
});

describe("test_transform_invalid_format_id", () => {
  it("errors when format_id is absent from transformations", () => {
    const schema: JsonValue = {
      cif_schema: {},
      transformations: { format_a: {} },
    };
    const r = toCif({ name: "Widget" }, schema, "nonexistent_format");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("Format 'nonexistent_format' not found");
    }
  });
});

describe("test_transform_string_api", () => {
  it("parses input strings and re-stringifies the result", () => {
    const source = JSON.stringify({ name: "Widget", price: 19.99 });
    const schema = JSON.stringify(basicProductSchema());
    const r = toCifString(source, schema, "format_a");
    expect(r.ok).toBe(true);
    if (r.ok) {
      const cif = JSON.parse(r.value) as { [k: string]: JsonValue };
      expect(cif.product_name).toBe("Widget");
      expect(cif.product_price).toBe(19.99);
    }
  });
});

describe("test_transform_string_invalid_json", () => {
  it("returns an error for invalid source JSON", () => {
    const r = toCifString("invalid json", "{}", "format_a");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("Invalid source JSON");
    }
  });
});

describe("transformToCif free-function alias", () => {
  it("matches Rust's backward-compat wrapper", () => {
    const r = transformToCif(
      { name: "Widget", price: 19.99 },
      basicProductSchema(),
      "format_a",
    );
    expect(r.ok).toBe(true);
  });
});

describe("edge cases not covered by Rust suite", () => {
  it("errors when 'cif_schema' is missing", () => {
    const r = toCif(
      {},
      { transformations: { f: {} } } as JsonValue,
      "f",
    );
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("'cif_schema' not defined");
    }
  });

  it("skips optional fields whose source_path is absent", () => {
    const schema: JsonValue = {
      cif_schema: {
        name: { type: "string", required: true },
        nickname: { type: "string" }, // optional
      },
      transformations: {
        f: {
          name: { source_path: "n", type: "string" },
          nickname: { source_path: "nick", type: "string" },
        },
      },
    };
    const r = toCif({ n: "Alice" }, schema, "f");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ name: "Alice" });
    }
  });

  it("source_path '.' in element rules takes the element itself", () => {
    const schema: JsonValue = {
      cif_schema: {
        tags: {
          type: "array",
          element: { value: { type: "string" } },
        },
      },
      transformations: {
        f: {
          tags: {
            source_path: "tags",
            type: "array",
            element: {
              value: { source_path: ".", type: "string" },
            },
          },
        },
      },
    };
    const r = toCif({ tags: ["red", "blue", "green"] }, schema, "f");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({
        tags: [{ value: "red" }, { value: "blue" }, { value: "green" }],
      });
    }
  });

  it("errors when an array field's source is not an array", () => {
    const schema: JsonValue = {
      cif_schema: {
        items: {
          type: "array",
          element: { sku: { type: "string" } },
        },
      },
      transformations: {
        f: {
          items: {
            source_path: "items",
            type: "array",
            element: { sku: { source_path: "sku", type: "string" } },
          },
        },
      },
    };
    const r = toCif({ items: "not-an-array" }, schema, "f");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("not an array");
    }
  });

  it("numeric path segments index into arrays", () => {
    const schema: JsonValue = {
      cif_schema: { first_sku: { type: "string", required: true } },
      transformations: {
        f: { first_sku: { source_path: "items.0.sku", type: "string" } },
      },
    };
    const r = toCif({ items: [{ sku: "X" }, { sku: "Y" }] }, schema, "f");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ first_sku: "X" });
    }
  });

  it("object fields with children build nested CIF objects", () => {
    const schema: JsonValue = {
      cif_schema: {
        supplier: {
          type: "object",
          required: true,
          children: {
            name: { type: "string", required: true },
            email: { type: "string" },
          },
        },
      },
      transformations: {
        f: {
          supplier: {
            source_path: "vendor",
            type: "object",
            children: {
              name: { source_path: "companyName", type: "string" },
              email: { source_path: "contact.email", type: "string" },
            },
          },
        },
      },
    };
    const r = toCif(
      { vendor: { companyName: "Acme", contact: { email: "a@acme.io" } } },
      schema,
      "f",
    );
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({
        supplier: { name: "Acme", email: "a@acme.io" },
      });
    }
  });

  it("errors when a required object child is missing", () => {
    const schema: JsonValue = {
      cif_schema: {
        supplier: {
          type: "object",
          children: { name: { type: "string", required: true } },
        },
      },
      transformations: {
        f: {
          supplier: {
            source_path: "vendor",
            type: "object",
            children: { name: { source_path: "companyName", type: "string" } },
          },
        },
      },
    };
    const r = toCif({ vendor: {} }, schema, "f");
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toContain("required children field 'supplier.name'");
    }
  });

  it("top-level source_path '.' collects root fields into a nested CIF object", () => {
    const schema: JsonValue = {
      cif_schema: {
        net_suite: {
          type: "object",
          children: { id: { type: "string" } },
        },
      },
      transformations: {
        f: {
          net_suite: {
            source_path: ".",
            type: "object",
            children: { id: { source_path: "id", type: "string" } },
          },
        },
      },
    };
    const r = toCif({ id: "42", other: true }, schema, "f");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ net_suite: { id: "42" } });
    }
  });

  it("source_path '.' on an object child takes the parent value itself", () => {
    const schema: JsonValue = {
      cif_schema: {
        meta: {
          type: "object",
          children: { raw: { type: "string" } },
        },
      },
      transformations: {
        f: {
          meta: {
            source_path: "tag",
            type: "object",
            children: { raw: { source_path: ".", type: "string" } },
          },
        },
      },
    };
    const r = toCif({ tag: "urgent" }, schema, "f");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({ meta: { raw: "urgent" } });
    }
  });

  it("objects inside array elements and arrays inside objects compose", () => {
    const schema: JsonValue = {
      cif_schema: {
        items: {
          type: "array",
          element: {
            sku: { type: "string" },
            dimensions: {
              type: "object",
              children: {
                width: { type: "number" },
                height: { type: "number" },
              },
            },
          },
        },
        supplier: {
          type: "object",
          children: {
            name: { type: "string" },
            addresses: {
              type: "array",
              element: { city: { type: "string" } },
            },
          },
        },
      },
      transformations: {
        f: {
          items: {
            source_path: "lines",
            type: "array",
            element: {
              sku: { source_path: "sku", type: "string" },
              dimensions: {
                source_path: "dims",
                type: "object",
                children: {
                  width: { source_path: "w", type: "number" },
                  height: { source_path: "h", type: "number" },
                },
              },
            },
          },
          supplier: {
            source_path: "vendor",
            type: "object",
            children: {
              name: { source_path: "name", type: "string" },
              addresses: {
                source_path: "addrs",
                type: "array",
                element: { city: { source_path: "city", type: "string" } },
              },
            },
          },
        },
      },
    };
    const r = toCif(
      {
        lines: [{ sku: "X", dims: { w: 2, h: 3 } }],
        vendor: { name: "Acme", addrs: [{ city: "Oslo" }, { city: "Bergen" }] },
      },
      schema,
      "f",
    );
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({
        items: [{ sku: "X", dimensions: { width: 2, height: 3 } }],
        supplier: { name: "Acme", addresses: [{ city: "Oslo" }, { city: "Bergen" }] },
      });
    }
  });

  it("nested arrays of objects are walked recursively", () => {
    const schema: JsonValue = {
      cif_schema: {
        orders: {
          type: "array",
          element: {
            id: { type: "string" },
            lines: {
              type: "array",
              element: { sku: { type: "string" } },
            },
          },
        },
      },
      transformations: {
        f: {
          orders: {
            source_path: "orders",
            type: "array",
            element: {
              id: { source_path: "id", type: "string" },
              lines: {
                source_path: "lines",
                type: "array",
                element: { sku: { source_path: "sku", type: "string" } },
              },
            },
          },
        },
      },
    };
    const r = toCif(
      {
        orders: [
          { id: "o1", lines: [{ sku: "a" }, { sku: "b" }] },
          { id: "o2", lines: [{ sku: "c" }] },
        ],
      },
      schema,
      "f",
    );
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toEqual({
        orders: [
          { id: "o1", lines: [{ sku: "a" }, { sku: "b" }] },
          { id: "o2", lines: [{ sku: "c" }] },
        ],
      });
    }
  });
});
