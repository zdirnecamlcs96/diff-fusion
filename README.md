# diff-fusion

A JSON diff tool with **Common Intermediate Format (CIF)** support for comparing structurally different JSON files.

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

- Compare JSON files with different structures
- Define transformers using a schema file
- Extract nested values using dot notation (`pricing.amount`)
- Automatic type conversion (string ↔ number ↔ boolean)
- Colored diff output

## Installation

```bash
git clone https://github.com/zdirnecamlcs96/diff-fusion.git
cd diff-fusion
cargo build --release
```

## Usage

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
cargo run -- diff a.json b.json \
  --schema schema.json \
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

See the `examples/` directory for more:

```bash
# Nested vs flat structures
cargo run -- diff examples/nested_a.json examples/nested_b.json \
  --schema examples/nested_schema.json \
  --format-a nested_format \
  --format-b flat_format
```

## License

MIT License - see [LICENSE](LICENSE) file
