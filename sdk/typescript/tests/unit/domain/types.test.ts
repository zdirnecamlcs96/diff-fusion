import { describe, expect, it } from "vitest";
import {
  type AnchorRole,
  type CifFieldDefinition,
  type CifType,
  type FieldTransformation,
  type JsonValue,
  cifFieldDefinition,
  cifTypeFromString,
  cifTypeToString,
  fieldTransformation,
  isCollection,
  isNullable,
  isPrimitive,
  toJsonSchema,
  validateFieldDefinition,
  validateFieldType,
  validateSchema,
} from "../../../src/domain/types.js";

describe("CifType", () => {
  it("parses from string (test_cif_type_from_str)", () => {
    expect(cifTypeFromString("string")).toBe("string");
    expect(cifTypeFromString("number")).toBe("number");
    expect(cifTypeFromString("boolean")).toBe("boolean");
    expect(cifTypeFromString("invalid")).toBeUndefined();
  });

  it("parses case-insensitively (matches Rust to_lowercase)", () => {
    expect(cifTypeFromString("STRING")).toBe("string");
    expect(cifTypeFromString("Number")).toBe("number");
  });

  it("converts to string (test_cif_type_display)", () => {
    expect(cifTypeToString("string")).toBe("string");
    expect(cifTypeToString("number")).toBe("number");
    expect(cifTypeToString("boolean")).toBe("boolean");
  });

  it("predicates (test_cif_type_predicates)", () => {
    expect(isPrimitive("string")).toBe(true);
    expect(isPrimitive("array")).toBe(false);
    expect(isCollection("array")).toBe(true);
    expect(isCollection("string")).toBe(false);
  });

  it("isNullable only for null", () => {
    expect(isNullable("null")).toBe(true);
    expect(isNullable("string")).toBe(false);
  });

  it("all six variants parse", () => {
    const variants: CifType[] = [
      "string",
      "number",
      "boolean",
      "object",
      "array",
      "null",
    ];
    for (const v of variants) {
      expect(cifTypeFromString(v)).toBe(v);
    }
  });
});

describe("CifFieldDefinition", () => {
  it("required (test_field_definition_required)", () => {
    const field = cifFieldDefinition("string").required().build();
    expect(field.type).toBe("string");
    expect(field.required).toBe(true);
    expect(field.description).toBeUndefined();
  });

  it("optional (test_field_definition_optional)", () => {
    const field = cifFieldDefinition("number").optional().build();
    expect(field.type).toBe("number");
    expect(field.required).toBe(false);
  });

  it("with description (test_field_definition_with_description)", () => {
    const field = cifFieldDefinition("string")
      .required()
      .withDescription("Product name")
      .build();
    expect(field.description).toBe("Product name");
  });

  it("builder pattern chains (test_field_definition_builder_pattern)", () => {
    const field = cifFieldDefinition("string")
      .required()
      .withDescription("Email address")
      .withDefault("user@example.com")
      .build();

    expect(field.type).toBe("string");
    expect(field.required).toBe(true);
    expect(field.description).toBe("Email address");
    expect(field.default).toBe("user@example.com");
  });

  it("validates (test_field_definition_validate)", () => {
    const valid = cifFieldDefinition("string").required().build();
    expect(validateFieldDefinition(valid)).toEqual({ ok: true });

    const invalid: CifFieldDefinition = {
      type: "invalid_type",
      required: true,
    };
    const result = validateFieldDefinition(invalid);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toContain("Unsupported type");
    }
  });

  it("rejects required + default combination", () => {
    const field = cifFieldDefinition("string")
      .required()
      .withDefault("x")
      .build();
    const result = validateFieldDefinition(field);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toContain("Required fields");
    }
  });

  it("source_of_truth builder (test_source_of_truth)", () => {
    const field = cifFieldDefinition("number")
      .required()
      .withSourceOfTruth("inventory_system")
      .withDescription("Stock level")
      .build();
    expect(field.source_of_truth).toBe("inventory_system");
    expect(field.description).toBe("Stock level");
  });

  it("conflict_strategy builder (test_conflict_strategy)", () => {
    const field = cifFieldDefinition("number")
      .withConflictStrategy("last_write_wins")
      .withDescription("Price")
      .build();
    expect(field.conflict_strategy).toBe("last_write_wins");
  });

  it("with_element stores element schema", () => {
    const element: Record<string, CifFieldDefinition> = {
      id: cifFieldDefinition("string").required().build(),
      qty: cifFieldDefinition("number").required().build(),
    };
    const arr = cifFieldDefinition("array").withElement(element).build();
    expect(arr.element).toBeDefined();
    expect(arr.element?.id?.type).toBe("string");
    expect(arr.element?.qty?.type).toBe("number");
  });

  it("with_anchor marks a role", () => {
    const a = cifFieldDefinition("string").withAnchor("a").build();
    const b = cifFieldDefinition("string").withAnchor("b").build();
    expect(a.anchor).toBe("a");
    expect(b.anchor).toBe("b");
  });

  it("AnchorRole only has 'a' and 'b'", () => {
    const roles: AnchorRole[] = ["a", "b"];
    expect(roles).toEqual(["a", "b"]);
  });
});

describe("FieldTransformation", () => {
  it("constructs with notes (test_field_transformation)", () => {
    const t = fieldTransformation("name", "string")
      .withNotes("Maps product name")
      .build();
    expect(t.source_path).toBe("name");
    expect(t.type).toBe("string");
    expect(t.notes).toBe("Maps product name");
  });

  it("constructs without notes", () => {
    const t: FieldTransformation = fieldTransformation("price", "number").build();
    expect(t.source_path).toBe("price");
    expect(t.type).toBe("number");
    expect(t.notes).toBeUndefined();
  });
});

describe("Schema helpers", () => {
  it("to_json_schema + validate + field-type check (test_cif_schema_trait)", () => {
    const fields: ReadonlyArray<readonly [string, CifFieldDefinition]> = [
      [
        "email",
        cifFieldDefinition("string")
          .required()
          .withDescription("User email")
          .build(),
      ],
      ["age", cifFieldDefinition("number").optional().withDefault(0).build()],
    ];

    const json = toJsonSchema(fields) as { cif_schema: Record<string, JsonValue> };
    expect(json).toHaveProperty("cif_schema");
    expect(json.cif_schema).toHaveProperty("email");
    expect(json.cif_schema).toHaveProperty("age");

    const validData: JsonValue = {
      email: "test@example.com",
      age: 25,
    };
    expect(validateSchema(fields, validData)).toEqual({ ok: true });

    const missing: JsonValue = { age: 25 };
    const r1 = validateSchema(fields, missing);
    expect(r1.ok).toBe(false);
    if (!r1.ok) {
      expect(r1.errors[0]).toContain("Missing required field");
    }

    const wrongType: JsonValue = {
      email: "test@example.com",
      age: "not a number",
    };
    const r2 = validateSchema(fields, wrongType);
    expect(r2.ok).toBe(false);
    if (!r2.ok) {
      expect(r2.errors[0]).toContain("Expected type 'number'");
    }
  });

  it("validateSchema rejects non-object (matches Rust)", () => {
    const r = validateSchema([], 42 as unknown as JsonValue);
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.errors[0]).toBe("Value must be an object");
    }
  });
});

describe("validateFieldType (test_validate_field_type)", () => {
  it("matches types", () => {
    expect(validateFieldType("hello", "string")).toEqual({ ok: true });
    expect(validateFieldType(42, "number")).toEqual({ ok: true });
    expect(validateFieldType(true, "boolean")).toEqual({ ok: true });
    expect(validateFieldType({}, "object")).toEqual({ ok: true });
    expect(validateFieldType([], "array")).toEqual({ ok: true });
    expect(validateFieldType(null, "null")).toEqual({ ok: true });
  });

  it("rejects mismatches", () => {
    const r1 = validateFieldType("hello", "number");
    expect(r1.ok).toBe(false);
    const r2 = validateFieldType(42, "string");
    expect(r2.ok).toBe(false);
  });
});

describe("JSON wire format parity with Rust serde", () => {
  it("AnchorRole serializes as lowercase letters", () => {
    const a: AnchorRole = "a";
    expect(JSON.stringify(a)).toBe('"a"');
  });

  it("CifType serializes as lowercase", () => {
    const t: CifType = "string";
    expect(JSON.stringify(t)).toBe('"string"');
  });

  it("CifFieldDefinition omits undefined optionals after build()", () => {
    const field = cifFieldDefinition("string").required().build();
    const json = JSON.parse(JSON.stringify(field)) as Record<string, unknown>;
    expect(json.type).toBe("string");
    expect(json.required).toBe(true);
    expect("description" in json).toBe(false);
    expect("default" in json).toBe(false);
    expect("source_of_truth" in json).toBe(false);
    expect("conflict_strategy" in json).toBe(false);
    expect("element" in json).toBe(false);
    expect("anchor" in json).toBe(false);
  });

  it("FieldTransformation uses `type` wire name", () => {
    const t = fieldTransformation("name", "string").build();
    const json = JSON.parse(JSON.stringify(t)) as Record<string, unknown>;
    expect(json.source_path).toBe("name");
    expect(json.type).toBe("string");
    expect("target_type" in json).toBe(false);
  });

  it("ConflictStrategy uses snake_case values", () => {
    const strategies = [
      "last_write_wins",
      "prefer_a",
      "prefer_b",
      "manual_resolve",
      "use_max",
      "use_min",
      "merge",
    ] as const;
    for (const s of strategies) {
      const field = cifFieldDefinition("number")
        .withConflictStrategy(s)
        .build();
      expect(field.conflict_strategy).toBe(s);
    }
  });
});
