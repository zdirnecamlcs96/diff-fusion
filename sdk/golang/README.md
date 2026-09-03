# diff-fusion/go

Go delivery of the diff-fusion Rust kernel: a `wasm32-wasip1` artifact
run by [wazero](https://wazero.io) (pure Go, zero cgo).

## Use

    import "github.com/zdirnecamlcs96/diff-fusion/sdk/golang/kernel"

    k, err := kernel.New(ctx)
    defer k.Close(ctx)
    cif, err := k.TransformToCIF(ctx, source, schema, formatID)      // step 1: ingest -> CIF
    changelog, err := k.ThreeWayDiff(ctx, ancestor, a, b)            // step 2: detect changes
    merged, err := k.Resolve(ctx, ancestor, changelog, policyDoc, mergeCtx) // step 3: resolve -> {"value":...,"conflicts":[...]}
    out, err := k.TransformFromCIF(ctx, mergedValue, schema, formatID) // step 4: emit CIF -> source ("value" from step 3)

The four-step pipeline is `TransformToCIF` -> `ThreeWayDiff` -> `Resolve` ->
`TransformFromCIF`, plus `Close`. Wire contract (wire shapes, policy config,
merge outcome) is pinned by `../../spec/schema/`; conformance by
`../../spec/vectors/` — the Rust generator is the sole producer.

A `Kernel` is not goroutine-safe; create one per goroutine or pool.

## Regenerate the artifact

From the repo root (rustup-managed cargo, `wasm32-wasip1` target installed):

    ./scripts/build-wasm-wasip1.sh
    cd go && go test ./...

The orchestrator/adapter layers are deliberately not here — app-layer code
is native per host (see ROADMAP "Architecture"). This package is kernel
delivery only.

## SchemaFromStruct

`kernel.SchemaFromStruct` derives the CIF schema JSON that `TransformToCIF`
expects from a tagged Go struct, instead of hand-writing schema.json:

    type Item struct {
        SKU string  `cif:"sku,required" hubspot:"properties.sku" salesforce:"StockKeepingUnit"`
        Qty float64 `cif:"qty" hubspot:"properties.quantity"`   // no salesforce tag: local-only there
    }
    type Doc struct {
        Items []Item `cif:"items" hubspot:"lineItems" salesforce:"."`
    }

    schema, err := kernel.SchemaFromStruct(Doc{}, "hubspot", "salesforce")
    out, err := k.TransformToCIF(ctx, source, schema, "hubspot")

`cif:"<field>[,required]"` names the CIF field (`cif:"-"` or no tag skips
it); each format passed to `SchemaFromStruct` reads its source path from the
struct tag of that same name, so a field can be mapped in one format and
omitted (local-only) in another.

What it emits (pretty-printed here for readability; the real call produces
compact JSON) for the struct above:

    {
      "cif_schema": {
        "items": {
          "type": "array",
          "element": {
            "qty": { "type": "number" },
            "sku": { "type": "string", "required": true }
          }
        }
      },
      "transformations": {
        "hubspot": {
          "items": {
            "source_path": "lineItems",
            "type": "array",
            "element": {
              "qty": { "source_path": "properties.quantity", "type": "number" },
              "sku": { "source_path": "properties.sku", "type": "string" }
            }
          }
        },
        "salesforce": {
          "items": {
            "source_path": ".",
            "type": "array",
            "element": {
              "sku": { "source_path": "StockKeepingUnit", "type": "string" }
            }
          }
        }
      }
    }

A source path of `"."` means "the current scope" — the kernel convention
used when a format's substructure already matches the enclosing element.

- Nested objects must be a struct with cif-tagged fields — map fields are
  rejected, and a struct with zero cif-tagged fields is rejected (an opaque
  `{"type":"object"}` with no declared schema isn't allowed).
- Time fields must be declared as `string` holding a UTC RFC 3339 timestamp;
  `time.Time` fields are rejected.
- `any`/interface fields are rejected — declare a concrete schema type.
- Types implementing `json.Marshaler` are rejected — reflection can't see
  their custom JSON shape; declare the field with the marshaled type instead
  (e.g. `string`).
- Duplicate `cif` field names within the same struct are rejected.
