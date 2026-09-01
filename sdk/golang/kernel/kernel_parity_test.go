package kernel

import (
	"context"
	"sort"
	"strings"
	"testing"

	"github.com/tetratelabs/wazero"
)

// wantExports is the full set of df_*-prefixed exports the kernel wasm
// artifact is expected to have. New kernel export? Bind it in kernel.go,
// then add it here. Hardcoded on purpose — see kernel_vectors_test.go.
var wantExports = []string{
	"df_alloc",
	"df_dealloc",
	"df_three_way_diff",
	"df_merge_field",
	"df_canonical_json",
	"df_compare_json",
	"df_transform_to_cif",
	"df_idempotency_key_hex",
	"df_merge_batch",
	"df_fuse",
}

// TestKernelExportParity fails the moment the Rust kernel's df_* export
// surface drifts from what this SDK knows about, in either direction:
// a new export nothing binds, or a bound export the artifact dropped.
func TestKernelExportParity(t *testing.T) {
	ctx := context.Background()
	r := wazero.NewRuntime(ctx)
	defer r.Close(ctx)

	compiled, err := r.CompileModule(ctx, wasmBytes)
	if err != nil {
		t.Fatalf("CompileModule: %v", err)
	}

	var got []string
	for name := range compiled.ExportedFunctions() {
		if strings.HasPrefix(name, "df_") {
			got = append(got, name)
		}
	}
	sort.Strings(got)

	want := append([]string(nil), wantExports...)
	sort.Strings(want)

	if len(got) != len(want) {
		t.Fatalf("df_* export set mismatch\ngot:  %v\nwant: %v", got, want)
	}
	for i := range got {
		if got[i] != want[i] {
			t.Fatalf("df_* export set mismatch\ngot:  %v\nwant: %v", got, want)
		}
	}
}
