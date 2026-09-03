# examples

App-layer glue over the diff-fusion kernel — not part of the `kernel` package.

## jobs

Four pure jobs, one per kernel step:

1. `TransformIn[D, E any](format string, entity E) (CIF, error)` — entity -> CIF (`kernel.TransformToCIF`)
2. `Detect(ancestor, a, b CIF) (Changelog, error)` — ancestor/a/b CIF -> changelog (`kernel.ThreeWayDiff`)
3. `Resolve(in ResolveInput) (ResolveOutput, error)` — changelog + policy -> merged CIF + conflicts (`kernel.Resolve`)
4. `TransformOut[D, E any](format string, cif CIF, into E) (E, error)` — CIF -> entity (`kernel.TransformFromCIF`)

CIF and Changelog are distinct types, so steps cannot be wired out of order.

## Run

    go run ./examples/pipeline
    go test ./examples/...
