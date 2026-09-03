// Package jobs is app-layer glue over the kernel package: the diff-fusion
// pipeline as four pure jobs, one per kernel step. Pure = deterministic, no
// I/O, no hidden state beyond the kernel instance, no time.Now.
//
//  1. TransformIn   entity -> CIF                                   (kernel.TransformToCIF)
//  2. Detect        ancestor/a/b CIF -> Changelog                   (kernel.ThreeWayDiff)
//  3. Resolve       Changelog + policy -> merged CIF + conflicts    (kernel.Resolve)
//  4. TransformOut  CIF -> entity                                   (kernel.TransformFromCIF)
//
// CIF and Changelog are distinct types, so a step handed the wrong step's output fails to compile.
package jobs

import (
	"context"
	"encoding/json"
	"sync"

	"github.com/zdirnecamlcs96/diff-fusion/sdk/golang/kernel"
)

// kernel built once per process. Kernel is not goroutine-safe (see kernel README), so
// every call goes through mu.
// ponytail: global lock; switch to per-goroutine kernels if throughput matters.
var k = sync.OnceValue(func() *kernel.Kernel {
	kk, err := kernel.New(context.Background())
	if err != nil {
		panic(err)
	}
	return kk
})
var mu sync.Mutex

// CIF is a Common Intermediate Format document. Embedding json.RawMessage promotes
// its MarshalJSON/UnmarshalJSON, so CIF passes through encoding/json unchanged while
// staying a distinct type from Changelog: a step handed the wrong step's output fails to compile.
type CIF struct{ json.RawMessage }

// Changelog is Detect's output and Resolve's input ({"changes":[...]}).
type Changelog struct{ json.RawMessage }

// TransformIn: step 1. entity -> CIF. E is the caller's own struct, already
// used for its system's json.Marshal/Unmarshal; its cif tags derive the
// schema via kernel.SchemaFromStruct(new(E), format).
func TransformIn[E any](format string, entity E) (CIF, error) {
	schema, err := kernel.SchemaFromStruct(new(E), format)
	if err != nil {
		return CIF{}, err
	}
	src, err := json.Marshal(entity)
	if err != nil {
		return CIF{}, err
	}
	mu.Lock()
	defer mu.Unlock()
	out, err := k().TransformToCIF(context.Background(), src, schema, format)
	if err != nil {
		return CIF{}, err
	}
	return CIF{RawMessage: out}, nil
}

// Detect: step 2. Three CIF documents -> Changelog, via kernel.ThreeWayDiff.
func Detect(ancestor, a, b CIF) (Changelog, error) {
	mu.Lock()
	defer mu.Unlock()
	out, err := k().ThreeWayDiff(context.Background(), ancestor.RawMessage, a.RawMessage, b.RawMessage)
	if err != nil {
		return Changelog{}, err
	}
	return Changelog{RawMessage: out}, nil
}

type ResolveInput struct {
	Ancestor         CIF
	Changelog        Changelog
	Policy           json.RawMessage // {"fields":{...}}
	SystemA, SystemB string
}

type ResolveOutput struct {
	Value     CIF             `json:"value"`
	Conflicts json.RawMessage `json:"conflicts"`
}

// Resolve: step 3. Applies policy to the changelog onto ancestor, via kernel.Resolve.
func Resolve(in ResolveInput) (ResolveOutput, error) {
	mergeCtx, err := json.Marshal(struct {
		SystemA string `json:"system_a"`
		SystemB string `json:"system_b"`
	}{in.SystemA, in.SystemB})
	if err != nil {
		return ResolveOutput{}, err
	}
	mu.Lock()
	out, err := k().Resolve(context.Background(), in.Ancestor.RawMessage, in.Changelog.RawMessage, in.Policy, mergeCtx)
	mu.Unlock()
	if err != nil {
		return ResolveOutput{}, err
	}
	var res ResolveOutput
	if err := json.Unmarshal(out, &res); err != nil {
		return ResolveOutput{}, err
	}
	return res, nil
}

// TransformOut: step 4. CIF -> entity. E is the caller's own struct (same one
// TransformIn used); its cif tags derive the schema via
// kernel.SchemaFromStruct(new(E), format). Applies the mapped source paths onto
// `into` and returns it; unmapped fields on `into` are untouched (update-not-replace).
// Via kernel.TransformFromCIF then json.Unmarshal.
func TransformOut[E any](format string, cif CIF, into E) (E, error) {
	schema, err := kernel.SchemaFromStruct(new(E), format)
	if err != nil {
		return into, err
	}
	mu.Lock()
	out, err := k().TransformFromCIF(context.Background(), cif.RawMessage, schema, format)
	mu.Unlock()
	if err != nil {
		return into, err
	}
	if err := json.Unmarshal(out, &into); err != nil {
		return into, err
	}
	return into, nil
}
