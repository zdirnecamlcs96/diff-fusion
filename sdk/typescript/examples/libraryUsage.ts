// Example: Using diff-fusion as a TypeScript library.
//
// Mirrors `examples/rust_library_usage.rs` in the Rust crate. Shows the
// programmatic primitives that sit underneath the `DiffFusion` facade —
// the Tier-0 `toCif` transformer, the `compareJson` two-way comparator,
// and the schema helpers (`cifFieldDefinition`, `toJsonSchema`,
// `validateSchema`). Each numbered example is independent and can be
// copy-pasted into a larger app as-is.
//
// Run from the `ts/` directory:
//
//   npx tsx examples/libraryUsage.ts

import { compareJson } from "../src/domain/compare.js";
import {
  cifFieldDefinition,
  cifTypeFromString,
  isPrimitive,
  toJsonSchema,
  validateSchema,
  type CifFieldDefinition,
  type JsonValue,
  type SchemaFields,
} from "../src/domain/types.js";
import { toCif } from "../src/application/transform.js";

console.log("🚀 diff-fusion Library Examples\n");

example1ValueApi();
example2StringApi();
example3RealWorld();
example4BuilderPattern();
example5TraitSchema();

// ---------------------------------------------------------------------------
// Example 1: Value API (idiomatic) — transform a source JSON into CIF.
// ---------------------------------------------------------------------------
function example1ValueApi(): void {
  console.log("📝 Example 1: Value API (idiomatic)\n");

  const source: JsonValue = { name: "Widget", price: 19.99 };

  const schema: JsonValue = {
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

  const result = toCif(source, schema, "format_a");
  if (result.ok) {
    console.log(`✅ CIF: ${JSON.stringify(result.value, null, 2)}\n`);
  } else {
    console.log(`❌ Error: ${result.error}\n`);
  }
}

// ---------------------------------------------------------------------------
// Example 2: String API (FFI-compatible shape).
// ---------------------------------------------------------------------------
function example2StringApi(): void {
  console.log("📝 Example 2: String API (FFI-compatible)\n");

  const sourceJson = '{"id": "P123", "stock": 100}';
  const schemaJson = `{
    "cif_schema": {
      "product_id": {"type": "string", "required": true},
      "quantity": {"type": "number", "required": true}
    },
    "transformations": {
      "system_a": {
        "product_id": {"source_path": "id", "type": "string"},
        "quantity": {"source_path": "stock", "type": "number"}
      }
    }
  }`;

  const source = JSON.parse(sourceJson) as JsonValue;
  const schema = JSON.parse(schemaJson) as JsonValue;
  const result = toCif(source, schema, "system_a");
  if (result.ok) {
    console.log(`✅ CIF JSON:\n${JSON.stringify(result.value)}\n`);
  } else {
    console.log(`❌ Error: ${result.error}\n`);
  }
}

// ---------------------------------------------------------------------------
// Example 3: Real-world — transform two systems to CIF and compare.
// ---------------------------------------------------------------------------
function example3RealWorld(): void {
  console.log("🌐 Example 3: Real-world API sync\n");

  const dbState: JsonValue = {
    id: "P123",
    stock: 50,
    updated_at: "2025-11-04T10:00:00Z",
  };
  const externalData: JsonValue = {
    product_id: "P123",
    inventory_quantity: 45,
    last_updated: "2025-11-04T10:30:00Z",
  };

  const schema: JsonValue = {
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
        quantity: { source_path: "inventory_quantity", type: "number" },
      },
    },
  };

  const cifA = toCif(dbState, schema, "system_a");
  const cifB = toCif(externalData, schema, "system_b");
  if (!cifA.ok || !cifB.ok) {
    console.log("❌ Transform failed, aborting example 3\n");
    return;
  }

  const diffs = compareJson(cifA.value, cifB.value);
  if (diffs.length === 0) {
    console.log("✅ No conflicts — systems are in sync!\n");
  } else {
    console.log("⚠️  Conflicts detected:");
    for (const [path, [oldVal, newVal]] of diffs) {
      console.log(
        `   ${path}: DB(${JSON.stringify(oldVal)}) vs External(${JSON.stringify(newVal)})`,
      );
      if (path === "quantity") {
        console.log("   ✓ Resolution: use external value (source of truth)");
      }
    }
    console.log();
  }
}

// ---------------------------------------------------------------------------
// Example 4: Builder pattern for CifFieldDefinition.
// ---------------------------------------------------------------------------
function example4BuilderPattern(): void {
  console.log("📝 Example 4: Builder pattern for CIF schema\n");

  const emailField = cifFieldDefinition("string")
    .required()
    .withDescription("User's email address")
    .build();

  const ageField = cifFieldDefinition("number")
    .optional()
    .withDescription("User's age")
    .withDefault(0)
    .build();

  const activeField = cifFieldDefinition("boolean")
    .required()
    .withDescription("Whether the user account is active")
    .build();

  printField("Email", emailField);
  printField("Age", ageField);
  printField("Active", activeField);

  const parsed = cifTypeFromString(emailField.type);
  if (parsed !== undefined) {
    console.log(`📝 Parsed CIF type: ${parsed}`);
    console.log(`   Is primitive? ${isPrimitive(parsed)}`);
  }
  console.log();
}

function printField(label: string, def: CifFieldDefinition): void {
  console.log(`✅ ${label} field definition:`);
  console.log(`   Type: ${def.type}`);
  console.log(`   Required: ${def.required}`);
  if (def.description !== undefined) {
    console.log(`   Description: ${def.description}`);
  }
  if (def.default !== undefined) {
    console.log(`   Default: ${JSON.stringify(def.default)}`);
  }
  console.log();
}

// ---------------------------------------------------------------------------
// Example 5: Schema-fields pattern — the TS analogue of Rust's `Schema`
// trait. CLAUDE.md §4 calls this out explicitly: TS passes the field list
// directly as a `SchemaFields` tuple array.
// ---------------------------------------------------------------------------
function example5TraitSchema(): void {
  console.log("🎯 Example 5: Schema-fields pattern (typed schema)\n");

  const productFields: SchemaFields = [
    [
      "product_id",
      cifFieldDefinition("string")
        .required()
        .withDescription("Unique product identifier")
        .build(),
    ],
    [
      "name",
      cifFieldDefinition("string")
        .required()
        .withDescription("Product name")
        .build(),
    ],
    [
      "price",
      cifFieldDefinition("number")
        .required()
        .withDescription("Product price in USD")
        .build(),
    ],
    [
      "in_stock",
      cifFieldDefinition("boolean")
        .optional()
        .withDescription("Stock availability")
        .withDefault(true)
        .build(),
    ],
  ];

  const jsonSchema = toJsonSchema(productFields);
  console.log("✅ Generated JSON schema:");
  console.log(`${JSON.stringify(jsonSchema, null, 2)}\n`);

  const validProduct: JsonValue = {
    product_id: "P123",
    name: "Laptop",
    price: 999.99,
    in_stock: true,
  };
  reportValidation("Valid product", productFields, validProduct);

  const missingRequired: JsonValue = {
    product_id: "P456",
    name: "Mouse",
  };
  reportValidation("Missing required field", productFields, missingRequired);

  const wrongType: JsonValue = {
    product_id: "P789",
    name: "Keyboard",
    price: "not a number",
  };
  reportValidation("Wrong type", productFields, wrongType);

  console.log("\n💡 Schema-fields pattern gives you:");
  console.log("   • Compile-time type safety via `SchemaFields`");
  console.log("   • IDE autocomplete for field names");
  console.log("   • Automatic JSON schema generation (`toJsonSchema`)");
  console.log("   • Runtime validation (`validateSchema`)");
  console.log();
}

function reportValidation(
  label: string,
  fields: SchemaFields,
  value: JsonValue,
): void {
  const result = validateSchema(fields, value);
  if (result.ok) {
    console.log(`✅ ${label}: passed validation`);
  } else {
    console.log(`⚠️  ${label}:`);
    for (const err of result.errors) {
      console.log(`   - ${err}`);
    }
  }
}
