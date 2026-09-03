// Package jobs is app-layer glue over the kernel package: the diff-fusion
// pipeline as four pure jobs, one per kernel step. Pure = deterministic, no
// I/O, no hidden state beyond the kernel instance, no time.Now.
//
//  1. TransformIn   entity -> CIF                                   (kernel.TransformToCIF)
//  2. Detect        ancestor/a/b CIF -> Changelog                   (kernel.ThreeWayDiff)
//  3. Resolve       Changelog + policy -> merged CIF + conflicts    (kernel.Resolve)
//  4. TransformOut  CIF -> entity                                   (kernel.TransformFromCIF)
//
// CIF and Changelog are distinct types, so a step given the wrong step's output fails to compile.
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

// CIF is a Common Intermediate Format document: output of TransformIn, input of Detect/Resolve/TransformOut.
type CIF json.RawMessage

// Changelog is Detect's output and Resolve's input ({"changes":[...]}).
type Changelog json.RawMessage

// UnmarshalJSON keeps the raw bytes: a defined type does not inherit json.RawMessage's methods, and
// Resolve decodes {"value":...} into a CIF.
func (c *CIF) UnmarshalJSON(b []byte) error {
	*c = append((*c)[:0], b...)
	return nil
}

// TransformIn: step 1. entity -> CIF. D = cif-tagged doc struct (schema via kernel.SchemaFromStruct(new(D), format)), E = entity type.
func TransformIn[D, E any](format string, entity E) (CIF, error) {
	schema, err := kernel.SchemaFromStruct(new(D), format)
	if err != nil {
		return nil, err
	}
	src, err := json.Marshal(entity)
	if err != nil {
		return nil, err
	}
	mu.Lock()
	defer mu.Unlock()
	out, err := k().TransformToCIF(context.Background(), src, schema, format)
	if err != nil {
		return nil, err
	}
	return CIF(out), nil
}

// Detect: step 2. Three CIF documents -> Changelog, via kernel.ThreeWayDiff.
func Detect(ancestor, a, b CIF) (Changelog, error) {
	mu.Lock()
	defer mu.Unlock()
	out, err := k().ThreeWayDiff(context.Background(), ancestor, a, b)
	if err != nil {
		return nil, err
	}
	return Changelog(out), nil
}

type ResolveInput struct {
	Ancestor         CIF
	Changelog        Changelog
	Policy           []byte // {"fields":{...}}
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
	out, err := k().Resolve(context.Background(), in.Ancestor, in.Changelog, in.Policy, mergeCtx)
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

// TransformOut: step 4. CIF -> entity. Applies the mapped source paths onto `into` and returns it;
// unmapped fields on `into` are untouched (update-not-replace). Via kernel.TransformFromCIF then json.Unmarshal.
func TransformOut[D, E any](format string, cif CIF, into E) (E, error) {
	schema, err := kernel.SchemaFromStruct(new(D), format)
	if err != nil {
		return into, err
	}
	mu.Lock()
	out, err := k().TransformFromCIF(context.Background(), cif, schema, format)
	mu.Unlock()
	if err != nil {
		return into, err
	}
	if err := json.Unmarshal(out, &into); err != nil {
		return into, err
	}
	return into, nil
}
