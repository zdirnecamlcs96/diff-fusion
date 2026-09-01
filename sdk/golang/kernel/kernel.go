// Package kernel delivers the diff-fusion Rust kernel to Go via a
// wasm32-wasip1 artifact run by wazero (pure Go, zero cgo). The wire
// contract is the repo's spec/schema/; all inputs and outputs are JSON.
// Regenerate the embedded artifact with scripts/build-wasm-wasip1.sh.
package kernel

import (
	"context"
	_ "embed"
	"encoding/json"
	"errors"
	"fmt"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

//go:embed diff_fusion.wasm
var wasmBytes []byte

// Kernel is one instantiated WASM module. Not goroutine-safe.
// ponytail: single instance, add a pool if contention shows.
type Kernel struct {
	runtime wazero.Runtime
	module  api.Module
	alloc   api.Function
	dealloc api.Function
	fns     map[string]api.Function
}

// API is the Kernel's exported surface, for host apps to mock in tests
// without instantiating the wazero/WASM runtime.
type API interface {
	Close(ctx context.Context) error
	Fuse(ctx context.Context, ancestor, a, b, policyDoc, mergeCtx []byte) ([]byte, error)
	TransformToCIF(ctx context.Context, source, schema []byte, formatID string) ([]byte, error)
}

var _ API = (*Kernel)(nil)

func New(ctx context.Context) (*Kernel, error) {
	r := wazero.NewRuntime(ctx)
	wasi_snapshot_preview1.MustInstantiate(ctx, r)
	mod, err := r.Instantiate(ctx, wasmBytes)
	if err != nil {
		r.Close(ctx)
		return nil, fmt.Errorf("instantiate kernel: %w", err)
	}
	// A wasip1 cdylib is a reactor: run its initializer if present.
	if init := mod.ExportedFunction("_initialize"); init != nil {
		if _, err := init.Call(ctx); err != nil {
			r.Close(ctx)
			return nil, fmt.Errorf("_initialize: %w", err)
		}
	}
	k := &Kernel{runtime: r, module: mod, fns: map[string]api.Function{}}
	k.alloc = mod.ExportedFunction("df_alloc")
	k.dealloc = mod.ExportedFunction("df_dealloc")
	if k.alloc == nil || k.dealloc == nil {
		r.Close(ctx)
		return nil, errors.New("kernel artifact missing df_alloc/df_dealloc")
	}
	for _, name := range []string{
		"df_three_way_diff", "df_merge_field", "df_canonical_json", "df_idempotency_key_hex",
		"df_compare_json", "df_transform_to_cif", "df_merge_batch", "df_fuse",
	} {
		fn := mod.ExportedFunction(name)
		if fn == nil {
			r.Close(ctx)
			return nil, fmt.Errorf("kernel artifact missing export %s", name)
		}
		k.fns[name] = fn
	}
	return k, nil
}

func (k *Kernel) Close(ctx context.Context) error { return k.runtime.Close(ctx) }

func (k *Kernel) threeWayDiff(ctx context.Context, ancestor, a, b []byte) ([]byte, error) {
	return k.call(ctx, "df_three_way_diff", ancestor, a, b)
}

func (k *Kernel) mergeField(ctx context.Context, change, policyRef, mergeCtx []byte) ([]byte, error) {
	return k.call(ctx, "df_merge_field", change, policyRef, mergeCtx)
}

func (k *Kernel) canonicalJSON(ctx context.Context, doc []byte) ([]byte, error) {
	return k.call(ctx, "df_canonical_json", doc)
}

func (k *Kernel) compareJSON(ctx context.Context, a, b []byte) ([]byte, error) {
	return k.call(ctx, "df_compare_json", a, b)
}

func (k *Kernel) TransformToCIF(ctx context.Context, source, schema []byte, formatID string) ([]byte, error) {
	return k.call(ctx, "df_transform_to_cif", source, schema, []byte(formatID))
}

func (k *Kernel) idempotencyKeyHex(ctx context.Context, canonicalID, operation string, payload []byte) (string, error) {
	out, err := k.call(ctx, "df_idempotency_key_hex", []byte(canonicalID), []byte(operation), payload)
	return string(out), err
}

func (k *Kernel) mergeBatch(ctx context.Context, changelog, policyDoc, mergeCtx []byte) ([]byte, error) {
	return k.call(ctx, "df_merge_batch", changelog, policyDoc, mergeCtx)
}

// Fuse three-way merges ancestor/a/b under policyDoc, returning the merged
// document and any conflicts.
func (k *Kernel) Fuse(ctx context.Context, ancestor, a, b, policyDoc, mergeCtx []byte) ([]byte, error) {
	return k.call(ctx, "df_fuse", ancestor, a, b, policyDoc, mergeCtx)
}

// envelope is the wasip1 result shape: exactly one of ok/err is set.
type envelope struct {
	Ok  *string `json:"ok"`
	Err *string `json:"err"`
}

// call writes each arg into guest memory (df_alloc), invokes the export
// with (ptr, len) pairs, reads back the packed (ptr<<32|len) envelope,
// and frees every guest buffer it caused to exist.
func (k *Kernel) call(ctx context.Context, name string, args ...[]byte) ([]byte, error) {
	type buf struct{ ptr, len uint32 }
	var argBufs []buf
	defer func() {
		for _, b := range argBufs {
			_, _ = k.dealloc.Call(ctx, uint64(b.ptr), uint64(b.len))
		}
	}()
	stack := make([]uint64, 0, len(args)*2)
	for _, a := range args {
		length := uint32(len(a))
		res, err := k.alloc.Call(ctx, uint64(length))
		if err != nil {
			return nil, fmt.Errorf("df_alloc: %w", err)
		}
		ptr := uint32(res[0])
		argBufs = append(argBufs, buf{ptr, length})
		if !k.module.Memory().Write(ptr, a) {
			return nil, errors.New("df_alloc buffer out of range")
		}
		stack = append(stack, uint64(ptr), uint64(length))
	}
	res, err := k.fns[name].Call(ctx, stack...)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", name, err)
	}
	outPtr, outLen := uint32(res[0]>>32), uint32(res[0])
	view, ok := k.module.Memory().Read(outPtr, outLen)
	if !ok {
		_, _ = k.dealloc.Call(ctx, uint64(outPtr), uint64(outLen))
		return nil, errors.New("result buffer out of range")
	}
	raw := make([]byte, len(view)) // copy: view invalidates on next guest call
	copy(raw, view)
	_, _ = k.dealloc.Call(ctx, uint64(outPtr), uint64(outLen))
	var env envelope
	if err := json.Unmarshal(raw, &env); err != nil {
		return nil, fmt.Errorf("bad kernel envelope: %w", err)
	}
	if env.Err != nil {
		return nil, errors.New(*env.Err)
	}
	if env.Ok == nil {
		return nil, errors.New("kernel envelope has neither ok nor err")
	}
	return []byte(*env.Ok), nil
}
