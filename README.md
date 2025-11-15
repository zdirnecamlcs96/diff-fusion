# diff-fusion

**A Rust library and CLI tool for detecting conflicts between different JSON formats.**

Think `git diff` for JSON data across multiple systems - transform to a common format (CIF), then compare and report differences. **You resolve conflicts manually**, just like Git.

> **Name Explained:** "fusion" = fusing different formats into CIF for comparison, not merging/syncing data

## What This Tool Does

```
System A JSON → CIF ← System B JSON
                 ↓
         Compare & Report
                 ↓
        "Field X differs"
                 ↓
         You Decide What To Do
```

**Like `git diff`:**

- ✅ Shows what changed
- ✅ Detects conflicts
- ❌ Does NOT merge automatically
- ❌ Does NOT push changes back

## The Problem

When integrating multiple systems, direct point-to-point transformations don't scale:

**Without CIF:**

- System A ↔ B, A ↔ C, A ↔ D, B ↔ C, B ↔ D, C ↔ D
- For **n systems**, you need **n(n-1)/2 transformers** (exponential growth!)
- Adding a new system requires updating multiple transformers
- Breaking changes cascade across all systems

**With CIF (This Tool):**

- System A → CIF, B → CIF, C → CIF, D → CIF
- For **n systems**, you need **n transformers** (linear growth!)
- Adding a new system requires only 1 new transformer
- Breaking changes isolated to a single transformer
- System A stays stable regardless of changes in B, C, or D

**Example:** Integrating with e-commerce platforms (Shopify, Amazon, etc.) - each has different JSON formats. Instead of writing A↔Shopify, A↔Amazon, A↔eBay transformers, you write A→CIF once, then CIF↔Shopify, CIF↔Amazon, CIF↔eBay separately.

## Features

- 🎯 **Simple Facade API** - Easy-to-use interface, no need to understand internals
- 🔄 **Transform** JSON from different formats to a common structure (CIF)
- ⚖️  **Compare** JSON data and detect conflicts automatically
- 🔍 **Report Differences** - Like `git diff`, shows what changed with detailed conflict info
- 👤 **Manual Resolution** - You decide how to handle conflicts (like Lodash, but with context)
- 📋 **Schema-driven** - Define transformations once, use everywhere
- 🔍 **Nested paths** - Extract values using dot notation (`user.profile.name`)
- 🔢 **Type conversion** - Automatic string ↔ number ↔ boolean conversion
- ✅ **Validation** - Built-in CIF schema validation
- 🎨 **Colored CLI** - Beautiful terminal output

## Quick Start (Library)

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
diff-fusion = { git = "https://github.com/zdirnecamlcs96/diff-fusion.git" }
serde_json = "1.0"
```

### Basic Usage

```rust
use diff_fusion::DiffFusion;
use serde_json::json;

fn main() {
    // 1. Define your schema once
    let schema = json!({
        "cif_schema": {
            "product_id": {"type": "string", "required": true},
            "product_name": {"type": "string", "required": true},
            "price": {"type": "number", "required": true}
        },
        "transformations": {
            "salesforce": {
                "product_id": {"source_path": "Id", "type": "string"},
                "product_name": {"source_path": "Name", "type": "string"},
                "price": {"source_path": "Price__c", "type": "number"}
            },
            "shopify": {
                "product_id": {"source_path": "id", "type": "string"},
                "product_name": {"source_path": "title", "type": "string"},
                "price": {"source_path": "variants.0.price", "type": "number"}
            }
        }
    });

    // 2. Create DiffFusion instance
    let diff_fusion = DiffFusion::new(schema);

    // 3. Transform data from different sources
    let salesforce_data = json!({"Id": "SF-001", "Name": "Widget", "Price__c": 29.99});
    let shopify_data = json!({"id": "SH-001", "title": "Widget", "variants": [{"price": 34.99}]});

    // 4. Transform to CIF and compare
    let report = diff_fusion
        .transform_and_compare(&salesforce_data, "salesforce", &shopify_data, "shopify")
        .unwrap();

    println!("Conflicts detected: {}", report.total_conflicts);
    for conflict in report.conflicts {
        println!("  {} changed: {} → {}", conflict.path, conflict.old_value, conflict.new_value);
    }
}
```

That's it! No need to understand transformers, comparators, or internal functions.

## CLI Usage

### 1. Create your JSON files

**a.json**

```json
{
  "name": "apple",
  "price": 5
}
```

**b.json**

```json
{
  "product_name": "apple",
  "cost": 5.5
}
```

### 2. Define a schema

**schema.json**

```json
{
  "cif_schema": {
    "product_name": {
      "type": "string",
      "required": true
    },
    "product_price": {
      "type": "number",
      "required": true
    }
  },
  "transformations": {
    "format_a": {
      "product_name": {
        "source_path": "name",
        "type": "string"
      },
      "product_price": {
        "source_path": "price",
        "type": "number"
      }
    },
    "format_b": {
      "product_name": {
        "source_path": "product_name",
        "type": "string"
      },
      "product_price": {
        "source_path": "cost",
        "type": "number"
      }
    }
  }
}
```

### 3. Run the comparison

```bash
cargo run -- diff tests/fixtures/a.json tests/fixtures/b.json \
  --schema tests/fixtures/schema.json \
  --format-a format_a \
  --format-b format_b
```

### Output

```
Transformed to CIF:
CIF A:
{
  "product_name": "apple",
  "product_price": 5
}

CIF B:
{
  "product_name": "apple",
  "product_price": 5.5
}

✗ Differences found:
  product_price: Number(5) → Number(5.5)
```

## How the Schema Works

The schema has two parts:

### 1. CIF Schema (the common format)

```json
"cif_schema": {
  "product_name": { "type": "string", "required": true },
  "product_price": { "type": "number", "required": true }
}
```

This defines what fields exist in the common format.

### 2. Transformations (how to map each source)

```json
"transformations": {
  "format_a": {
    "product_name": { "source_path": "name", "type": "string" },
    "product_price": { "source_path": "price", "type": "number" }
  },
  "format_b": {
    "product_name": { "source_path": "product_name", "type": "string" },
    "product_price": { "source_path": "cost", "type": "number" }
  }
}
```

This tells the tool where to find each field in the source files.

## Advanced Features

### Nested Paths

Use dot notation to extract nested values:

```json
{
  "source_path": "pricing.amount"
}
```

Works with:

```json
{
  "pricing": {
    "amount": 5
  }
}
```

### Array Access

Use index notation:

```json
{
  "source_path": "items.0.name"
}
```

### Type Conversion

Automatically converts between:

- String ↔ Number (`"5"` becomes `5`)
- String ↔ Boolean (`"true"` becomes `true`)
- Number ↔ Boolean (`0` becomes `false`)

## CLI Options

```bash
diff_fusion diff <file_a> <file_b> --schema <schema> [OPTIONS]

OPTIONS:
    --schema <schema>         Schema file (required)
    --format-a <format_a>     Format ID for file A [default: format_a]
    --format-b <format_b>     Format ID for file B [default: format_b]
```

## Examples

### CLI Examples

Sample data files are in `tests/fixtures/`:

```bash
# Basic comparison with different formats
cargo run -- diff tests/fixtures/a.json tests/fixtures/b.json \
  --schema tests/fixtures/schema.json \
  --format-a format_a \
  --format-b format_b

# Nested vs flat structures
cargo run -- diff tests/fixtures/nested_a.json tests/fixtures/nested_b.json \
  --schema tests/fixtures/nested_schema.json \
  --format-a nested_format \
  --format-b flat_format
```

### Library Examples

See `examples/rust_library_usage.rs` for how to use diff-fusion as a library:

```bash
cargo run --example rust_library_usage
```

## License

MIT License - see [LICENSE](LICENSE) file
