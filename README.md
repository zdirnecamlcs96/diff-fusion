# diff-fusion

**A Rust library for two-way reconciliation between authoritative systems.**

Transform each system's JSON to a canonical format (CIF), compute a
three-way diff against a stored ancestor, resolve per-field merge policies,
and push the result back — with optimistic concurrency and deterministic
idempotency keys. Unresolvable conflicts route to a review queue with full
provenance.

The library is built up in layers. The detection-only sub-use-case (the
`DiffFusion` facade below) still works standalone — use it if you want a
JSON diff without the reconciliation machinery.

> **Name Explained:** "fusion" = fusing different formats into CIF for comparison, not merging/syncing data blindly.

## Repository layout

Three delivery packages share one Rust kernel (see `ROADMAP.md`):

- `core/` — the Rust crate (kernel SSOT + full library). All `cargo`
  commands in this document run from `core/`.
- `sdk/typescript/` — npm package, kernel delivered via wasm-bindgen (`sdk/typescript/wasm/`).
- `sdk/golang/` — Go module, kernel delivered via wasm32-wasip1 + wazero.
- `spec/` — cross-language contract: golden vectors + boundary JSON Schema.
- `scripts/` — artifact build scripts (`build-wasm.sh`, `build-wasm-wasip1.sh`).

## What This Library Does

```
System A ─▶ Canonical ─┐
                       ├─ Three-way diff (A, B, ancestor)
System B ─▶ Canonical ─┘          │
                                  ▼
                              Resolve per-field policies
                                  │
                                  ├── clean: push stale side(s), commit ancestor LAST
                                  └── conflicts: route to escalation queue
```

**What you get:**

- ✅ Three-way diff with `source: A | B | Both` per field
- ✅ Declarative merge policies (`OwnedBy`, `Additive`, `Append`, `StateMachine`, `SetByKey`)
- ✅ Optimistic concurrency + idempotency keys on every push
- ✅ Escalation queue for the ~5% of conflicts that require human judgement
- ✅ Shadow mode — diff without pushing, for validating new adapters
- ✅ Conflict taxonomy (`NoPolicy` / `PolicyConflict` / `InvariantViolation`) so callers can branch disposition per class
- ✅ Durable filesystem-backed ancestor store (`adapters::filesystem_ancestor`)
- ✅ `SyncEngine` facade — one builder, no `Arc<dyn …>` ceremony; full layered module tree (`domain / application / ports / adapters / drivers`)
- ✅ The detection-only facade (`DiffFusion`) stays available as a Tier-0 entry point
- ✅ Passive `Observer` hook (`ports::observer`) + `diff_fusion_observe::HttpObserver` companion crate — stream pipeline events from your own program to any HTTP capture endpoint. See `examples/observe_demo.rs`.

**What it's *not*:** a workflow engine, a real-time event bus, a generic
ETL tool, or a CRDT. See `ROADMAP.md` for the out-of-scope list and why.

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
