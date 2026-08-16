---
layout: default
title: Go
parent: SDKs
nav_order: 3
---

# Go

Kernel delivery only: a `wasm32-wasip1` build of the Rust kernel, run by [wazero](https://wazero.io) (pure Go, zero cgo).

## Install

```bash
go get github.com/zdirnecamlcs96/diff-fusion/sdk/golang
```

```go
import "github.com/zdirnecamlcs96/diff-fusion/sdk/golang/kernel"
```

## Exported functions

Six functions total, on `kernel.Kernel` (`sdk/golang/kernel/kernel.go`):

```go
func New(ctx context.Context) (*Kernel, error)
func (k *Kernel) Close(ctx context.Context) error

func (k *Kernel) ThreeWayDiff(ctx context.Context, ancestor, a, b []byte) ([]byte, error)
func (k *Kernel) MergeField(ctx context.Context, change, policyRef, mergeCtx []byte) ([]byte, error)
func (k *Kernel) CanonicalJSON(ctx context.Context, doc []byte) ([]byte, error)
func (k *Kernel) IdempotencyKeyHex(ctx context.Context, canonicalID, operation string, payload []byte) (string, error)
```

```go
k, err := kernel.New(ctx)
defer k.Close(ctx)
out, err := k.ThreeWayDiff(ctx, ancestor, a, b) // JSON bytes in/out
```

## JSON-bytes-in/out contract

`ThreeWayDiff`, `MergeField`, and `CanonicalJSON` all take and return raw JSON bytes — no Go structs cross the boundary. The wire shapes (changelog, policy config, merge outcome) are pinned by the JSON Schemas in `spec/schema/`; conformance is checked against the golden vectors in `spec/vectors/` — `kernel-vectors.json` for `ThreeWayDiff`/`MergeField`, `idempotency-vectors.json` for `CanonicalJSON`/`IdempotencyKeyHex` — generated solely by the Rust examples and compared byte-exact, including error strings. `IdempotencyKeyHex` takes the canonical id and operation as plain strings plus the payload as JSON bytes, and returns the hex-encoded digest as a string — byte-identical to the Rust and TypeScript output for the same inputs.

## `set_by_key` policy config can't reach `Union`/`PreferA`/`PreferB`

The `set_by_key` policy declaration accepted by `MergeField`'s `policyRef` JSON only carries `identity`, `a_anchor`, and `b_anchor`. The Rust policy itself also has an `on_both_changed` setting (`Escalate` / `PreferA` / `PreferB` / `Union`), but the JSON constructor always defaults it to `Escalate` — there's currently no JSON shape that reaches the other three variants. Since Go has no native policy layer to construct the fuller struct directly, a Go caller using `set_by_key` always escalates when both sides change the same matched element. This is current behavior, not a guarantee — an open design question, not yet decided.
{: .warning }

## Not goroutine-safe

A `Kernel` is not goroutine-safe — create one per goroutine, or pool instances.
{: .warning }

## No orchestrator or adapter layer

Unlike the Rust and TypeScript deliveries, Go ships **kernel only**. There is no `SyncEngine`, no `SystemPort`, no ancestor store or escalation queue in this package — the orchestrator/adapter layers are deliberately not here (see the roadmap's architecture notes). If your Go service needs the full reconciliation pipeline, you build that app layer natively in Go and call the kernel's four primitives (`ThreeWayDiff`, `MergeField`, `CanonicalJSON`, `IdempotencyKeyHex`) from inside it.

## Regenerate the artifact

From the repo root (rustup-managed `cargo`, `wasm32-wasip1` target installed):

```bash
./scripts/build-wasm-wasip1.sh
cd sdk/golang && go test ./...
```
