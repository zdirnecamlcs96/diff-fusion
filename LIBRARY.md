# diff-fusion Library

A Rust library for JSON transformation and conflict detection using Common Intermediate Format (CIF). Designed for scalable system integration.

## Features

- ✅ **Schema-driven transformation**: Transform different JSON formats to a common format
- ✅ **Conflict detection**: Compare JSONs and detect differences with structured output
- ✅ **Cross-language support**: Use from Rust, TypeScript, Dart, Python, etc. via FFI
- ✅ **CLI tool included**: Command-line interface for quick operations
- ✅ **Type normalization**: Automatic type conversion (string, number, boolean)
- ✅ **Nested path support**: Extract values with dot notation (`pricing.amount`)

## Installation

### As Rust Library

```toml
[dependencies]
diff_fusion = { path = "/path/to/diff-fusion" }
```

### As CLI Tool

```bash
cargo install --path .
```

### For TypeScript

```bash
cd bindings/typescript
npm install
```

## Usage

### Rust Library

```rust
use diff_fusion::{transform_to_cif, compare_json, ConflictReport};
use serde_json::{json, Value};

// Transform JSON to CIF
let source = json!({"name": "Widget", "price": 19.99});
let schema = json!({
    "cif_schema": {
        "product_name": {"type": "string", "required": true},
        "product_price": {"type": "number", "required": true}
    },
    "transformations": {
        "format_a": {
            "product_name": {"source_path": "name", "type": "string"},
            "product_price": {"source_path": "price", "type": "number"}
        }
    }
});

let cif = transform_to_cif(&source, &schema, "format_a").unwrap();

// Compare CIFs
let cif_a = json!({"product_name": "Widget", "product_price": 19.99});
let cif_b = json!({"product_name": "Widget", "product_price": 24.99});
let diffs = compare_json(&cif_a, &cif_b);

// Or use string-based API for FFI compatibility
use diff_fusion::{transform_to_cif_string, compare_json_string};

let cif_json = transform_to_cif_string(
    r#"{"name": "Widget"}"#.to_string(),
    schema_str,
    "format_a".to_string()
).unwrap();

let report_json = compare_json_string(cif_a_str, cif_b_str).unwrap();
let report: ConflictReport = serde_json::from_str(&report_json).unwrap();
```

### CLI

```bash
# Compare two JSON files with different formats
diff-fusion diff a.json b.json \
  --schema schema.json \
  --format-a format_a \
  --format-b format_b
```

### TypeScript

See `bindings/typescript/README.md` for TypeScript usage.

## Architecture

This library solves the integration scaling problem using a **hub-and-spoke architecture**:

**Without CIF (Point-to-Point):**

- n systems = n(n-1)/2 transformers (exponential!)
- Adding System C requires multiple new transformers

**With CIF (Hub-and-Spoke):**

- n systems = n transformers (linear!)
- Adding System C requires only 1 transformer
- System A remains stable regardless of changes in B, C, or D

## Library Structure

```
diff-fusion/
├── src/
│   ├── lib.rs          # Library exports & FFI layer
│   ├── main.rs         # CLI entry point
│   ├── cli.rs          # CLI argument parsing
│   ├── transform.rs    # CIF transformation logic
│   └── compare.rs      # JSON comparison logic
├── bindings/
│   └── typescript/     # TypeScript FFI bindings
└── Cargo.toml          # Rust config (lib + bin)
```

## FFI Functions

The library exports three C-compatible functions for use in other languages:

```c
// Transform JSON to CIF
char* diff_fusion_transform_to_cif(
    const char* source_json,
    const char* schema_json,
    const char* format_id
);

// Compare two CIF JSONs
char* diff_fusion_compare_json(
    const char* cif_a,
    const char* cif_b
);

// Free memory allocated by Rust
void diff_fusion_free_string(char* s);
```

## Schema Format

```json
{
  "cif_schema": {
    "field_name": {
      "type": "string|number|boolean",
      "required": true|false
    }
  },
  "transformations": {
    "format_id": {
      "cif_field_name": {
        "source_path": "source.field.path",
        "type": "string|number|boolean"
      }
    }
  }
}
```

## Use Cases

### 1. E-commerce Integration

Transform product data from Shopify, Amazon, WooCommerce into a common format for comparison and sync.

### 2. Multi-System Sync

Synchronize data between your internal ERP (System A) and external platforms (B, C, D) using System A's format as CIF.

### 3. API Migration

Detect breaking changes when migrating between API versions.

### 4. Conflict Detection

Implement git-like conflict detection and resolution for data synchronization.

## Contributing

Built with ❤️ for scalable system integration.

## License

MIT
