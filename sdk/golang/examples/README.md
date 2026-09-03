# examples

App-layer glue over the diff-fusion kernel — not part of the `kernel` package.

## jobs

Four pure jobs, one per kernel step:

1. `TransformIn`  — entity -> CIF bytes (`kernel.TransformToCIF`)
2. `Detect`       — ancestor/a/b CIF -> changelog (`kernel.ThreeWayDiff`)
3. `Resolve`      — changelog + policy -> merged CIF + conflicts (`kernel.Resolve`)
4. `TransformOut` — CIF -> entity JSON patch (`kernel.TransformFromCIF`)

## Run

    go run ./examples/pipeline
    go test ./examples/...
