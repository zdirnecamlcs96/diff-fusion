// Package jobs is app-layer glue over the kernel package: the diff-fusion
// pipeline as four pure jobs, one per kernel step. Pure = deterministic, no
// I/O, no hidden state beyond the kernel instance, no time.Now.
//
//  1. TransformIn   entity -> CIF bytes                            (kernel.TransformToCIF)
//  2. Detect        ancestor/a/b CIF -> changelog                  (kernel.ThreeWayDiff)
//  3. Resolve       changelog + policy -> merged CIF + conflicts   (kernel.Resolve)
//  4. TransformOut  CIF -> entity JSON patch                       (kernel.TransformFromCIF)
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

// TransformIn: entity -> CIF bytes. D is the cif-tagged doc struct; schema derived from its tags.
func TransformIn[D any](format string, entity any) ([]byte, error) {
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
	return k().TransformToCIF(context.Background(), src, schema, format)
}

// Detect: step 2. Three CIF documents -> changelog bytes ({"changes":[...]}), via kernel.ThreeWayDiff.
func Detect(ancestor, a, b []byte) ([]byte, error) {
	mu.Lock()
	defer mu.Unlock()
	return k().ThreeWayDiff(context.Background(), ancestor, a, b)
}

type ResolveInput struct {
	Ancestor         []byte // CIF
	Changelog        []byte // from Detect
	Policy           []byte // {"fields":{...}}
	SystemA, SystemB string
}

type ResolveOutput struct {
	Value     json.RawMessage `json:"value"`
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

// TransformOut: CIF -> entity JSON patch holding only mapped source paths.
// Apply with json.Unmarshal(patch, &existing): unmapped fields stay untouched.
func TransformOut[D any](format string, cif []byte) ([]byte, error) {
	schema, err := kernel.SchemaFromStruct(new(D), format)
	if err != nil {
		return nil, err
	}
	mu.Lock()
	defer mu.Unlock()
	return k().TransformFromCIF(context.Background(), cif, schema, format)
}
