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
expects from your system's own native struct — the one you already
`json.Marshal`/`Unmarshal` — instead of hand-writing schema.json:

    type HubspotContact struct {
        Properties struct {
            Email string `json:"email" cif:"email,required"`
            Phone string `json:"phone" cif:"phone"`
        } `json:"properties"`          // no cif tag: transparent
        HsScore int `json:"hs_score"`  // no cif tag: local-only, skipped
    }

    schema, err := kernel.SchemaFromStruct(new(HubspotContact), "hubspot")
    out, err := k.TransformToCIF(ctx, source, schema, "hubspot")

`format` (here `"hubspot"`) is the `format_id` key `TransformToCIF`/
`TransformFromCIF` select by; it can't be empty or `"cif"` (reserved).

Each field's source path comes from its `json` tag name (or the exact Go
field name if there's no `json` tag — no case folding); `json:"-"` skips the
field entirely. A literal `.` or `\` in that key is escaped the same way
`core/src/domain/json_path.rs` does, so it survives as part of the key.

`cif:"<field>[,required]"` puts the field in the CIF document as
`<field>`; `cif:"-"` skips it. A struct/`*struct` field with **no** `cif`
tag is transparent: it isn't a CIF node itself, but its own fields are
walked as if declared on the parent, with source paths prefixed by its own
key (`properties.email` above) — that's how one native struct maps its
whole shape without repeating `cif:"-"` everywhere. A scalar or slice field
with no `cif` tag is simply local-only and skipped (`hs_score` above).

What it emits (pretty-printed here for readability; the real call produces
compact JSON) for the struct above:

    {
      "cif_schema": {
        "email": { "type": "string", "required": true },
        "phone": { "type": "string" }
      },
      "transformations": {
        "hubspot": {
          "email": { "source_path": "properties.email", "type": "string" },
          "phone": { "source_path": "properties.phone", "type": "string" }
        }
      }
    }

- Nested objects must be a `cif`-tagged struct with `cif`-tagged fields of
  its own — map fields are rejected, and a struct with zero `cif`-tagged
  fields is rejected (an opaque `{"type":"object"}` with no declared schema
  isn't allowed).
- Slice/array elements must be primitive scalars or `cif`-tagged structs; a
  slice with no `cif` tag is skipped (an array can't be flattened).
- Time fields must be declared as `string` holding a UTC RFC 3339 timestamp;
  `time.Time` fields are rejected.
- `any`/interface fields are rejected — declare a concrete schema type.
- Types implementing `json.Marshaler` are rejected — reflection can't see
  their custom JSON shape; declare the field with the marshaled type instead
  (e.g. `string`).
- Duplicate `cif` field names within the same CIF scope are rejected,
  including two different native fields landing on the same name via
  transparency.
- Embedded structs without a `json` tag are promoted like `encoding/json`
  does: their fields are walked with no extra path segment.
