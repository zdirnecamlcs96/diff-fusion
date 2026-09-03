# examples

App-layer glue over the diff-fusion kernel — not part of the `kernel` package.

## jobs

Four pure jobs, one per kernel step:

1. `TransformIn[E any](format string, entity E) (CIF, error)` — entity -> CIF (`kernel.TransformToCIF`); schema derives from `new(E)`
2. `Detect(ancestor, a, b CIF) (Changelog, error)` — ancestor/a/b CIF -> changelog (`kernel.ThreeWayDiff`)
3. `Resolve(in ResolveInput) (ResolveOutput, error)` — changelog + policy -> merged CIF + conflicts (`kernel.Resolve`)
4. `TransformOut[E any](format string, cif CIF, into E) (E, error)` — CIF -> entity (`kernel.TransformFromCIF`); schema derives from `new(E)`

`E` is the caller's own native struct: `json` tags are its wire shape, `cif`
tags derive the schema. CIF and Changelog embed json.RawMessage: distinct
types, stdlib JSON behaviour.

## schema

Focused on `kernel.SchemaFromStruct`: derives a CIF schema from a native
`HubspotContact` struct exercising every derivation rule (required,
optional, `json:"-"`, no-json-tag, transparent nested struct, cif-tagged
nested struct, slice of struct, untagged local-only field, a
`json.Marshaler` field asserted with a cif tag type override), feeds it to
`TransformToCIF`, and shows the `time.Time` rejection error.

## Run

    go run ./examples/pipeline
    go run ./examples/schema
    go test ./examples/...
