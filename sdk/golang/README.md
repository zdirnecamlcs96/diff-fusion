# diff-fusion/go

Go delivery of the diff-fusion Rust kernel: a `wasm32-wasip1` artifact
run by [wazero](https://wazero.io) (pure Go, zero cgo).

## Use

    import "github.com/zdirnecamlcs96/diff-fusion/sdk/golang/kernel"

    k, err := kernel.New(ctx)
    defer k.Close(ctx)
    out, err := k.ThreeWayDiff(ctx, ancestor, a, b) // JSON bytes in/out

Four functions: `ThreeWayDiff`, `MergeField`, `CanonicalJSON`,
`IdempotencyKeyHex`. Wire contract (wire shapes, policy config, merge
outcome) is pinned by `../../spec/schema/`; conformance by
`../../spec/vectors/` — the Rust generator is the sole producer.

A `Kernel` is not goroutine-safe; create one per goroutine or pool.

## Regenerate the artifact

From the repo root (rustup-managed cargo, `wasm32-wasip1` target installed):

    ./scripts/build-wasm-wasip1.sh
    cd go && go test ./...

The orchestrator/adapter layers are deliberately not here — app-layer code
is native per host (see ROADMAP "Architecture"). This package is kernel
delivery only.
