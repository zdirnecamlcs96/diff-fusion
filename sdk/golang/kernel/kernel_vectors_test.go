package kernel

import (
	"encoding/json"
	"os"
	"testing"
)

type threeWayVector struct {
	Name     string `json:"name"`
	Ancestor string `json:"ancestor"`
	A        string `json:"a"`
	B        string `json:"b"`
	Expected string `json:"expected"`
	IsErr    bool   `json:"isErr"`
}

type mergeFieldVector struct {
	Name      string `json:"name"`
	Change    string `json:"change"`
	PolicyRef string `json:"policyRef"`
	Ctx       string `json:"ctx"`
	Expected  string `json:"expected"`
	IsErr     bool   `json:"isErr"`
}

type compareJSONVector struct {
	Name     string `json:"name"`
	A        string `json:"a"`
	B        string `json:"b"`
	Expected string `json:"expected"`
	IsErr    bool   `json:"isErr"`
}

type transformToCifVector struct {
	Name     string `json:"name"`
	Source   string `json:"source"`
	Schema   string `json:"schema"`
	FormatID string `json:"formatId"`
	Expected string `json:"expected"`
	IsErr    bool   `json:"isErr"`
}

type transformFromCifVector struct {
	Name     string `json:"name"`
	Cif      string `json:"cif"`
	Schema   string `json:"schema"`
	FormatID string `json:"formatId"`
	Expected string `json:"expected"`
	IsErr    bool   `json:"isErr"`
}

type resolveVector struct {
	Name      string `json:"name"`
	Ancestor  string `json:"ancestor"`
	Changelog string `json:"changelog"`
	PolicyDoc string `json:"policyDoc"`
	Ctx       string `json:"ctx"`
	Expected  string `json:"expected"`
	IsErr     bool   `json:"isErr"`
}

type mergeBatchVector struct {
	Name      string `json:"name"`
	Changelog string `json:"changelog"`
	PolicyDoc string `json:"policyDoc"`
	Ctx       string `json:"ctx"`
	Expected  string `json:"expected"`
	IsErr     bool   `json:"isErr"`
}

type fuseVector struct {
	Name      string `json:"name"`
	Ancestor  string `json:"ancestor"`
	A         string `json:"a"`
	B         string `json:"b"`
	PolicyDoc string `json:"policyDoc"`
	Ctx       string `json:"ctx"`
	Expected  string `json:"expected"`
	IsErr     bool   `json:"isErr"`
}

type kernelVectors struct {
	ThreeWayDiff     []threeWayVector         `json:"threeWayDiff"`
	MergeField       []mergeFieldVector       `json:"mergeField"`
	CompareJSON      []compareJSONVector      `json:"compareJson"`
	TransformToCif   []transformToCifVector   `json:"transformToCif"`
	MergeBatch       []mergeBatchVector       `json:"mergeBatch"`
	Fuse             []fuseVector             `json:"fuse"`
	TransformFromCif []transformFromCifVector `json:"transformFromCif"`
	Resolve          []resolveVector          `json:"resolve"`
}

// The P4 gate for three_way_diff/merge_field: all vectors, read straight
// from spec/vectors/ (monorepo relative path — the Rust generator is the
// sole producer). Every field is a JSON-encoded string so all runtimes get
// byte-identical input; "expected" is the bare wire string (the ok/err
// envelope is stripped by Kernel.call before it reaches us), so success and
// error cases are both compared as exact strings, never structurally.
func TestKernelVectors(t *testing.T) {
	raw, err := os.ReadFile("../../../spec/vectors/kernel-vectors.json")
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var vs kernelVectors
	if err := json.Unmarshal(raw, &vs); err != nil {
		t.Fatalf("parse vectors: %v", err)
	}
	if len(vs.ThreeWayDiff) != 17 || len(vs.MergeField) != 29 || len(vs.CompareJSON) != 8 || len(vs.TransformToCif) != 13 || len(vs.MergeBatch) != 5 || len(vs.Fuse) != 7 || len(vs.TransformFromCif) != 10 || len(vs.Resolve) != 7 {
		t.Fatalf("expected 17 threeWayDiff + 29 mergeField + 8 compareJson + 13 transformToCif + 5 mergeBatch + 7 fuse + 10 transformFromCif + 7 resolve vectors, got %d + %d + %d + %d + %d + %d + %d + %d",
			len(vs.ThreeWayDiff), len(vs.MergeField), len(vs.CompareJSON), len(vs.TransformToCif), len(vs.MergeBatch), len(vs.Fuse), len(vs.TransformFromCif), len(vs.Resolve))
	}

	k, ctx := newKernel(t)

	for _, v := range vs.ThreeWayDiff {
		t.Run("threeWayDiff/"+v.Name, func(t *testing.T) {
			out, err := k.ThreeWayDiff(ctx, []byte(v.Ancestor), []byte(v.A), []byte(v.B))
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}

	for _, v := range vs.MergeField {
		t.Run("mergeField/"+v.Name, func(t *testing.T) {
			out, err := k.mergeField(ctx, []byte(v.Change), []byte(v.PolicyRef), []byte(v.Ctx))
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}

	for _, v := range vs.CompareJSON {
		t.Run("compareJson/"+v.Name, func(t *testing.T) {
			out, err := k.compareJSON(ctx, []byte(v.A), []byte(v.B))
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}

	for _, v := range vs.TransformToCif {
		t.Run("transformToCif/"+v.Name, func(t *testing.T) {
			out, err := k.TransformToCIF(ctx, []byte(v.Source), []byte(v.Schema), v.FormatID)
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}

	for _, v := range vs.MergeBatch {
		t.Run("mergeBatch/"+v.Name, func(t *testing.T) {
			out, err := k.mergeBatch(ctx, []byte(v.Changelog), []byte(v.PolicyDoc), []byte(v.Ctx))
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}

	for _, v := range vs.Fuse {
		t.Run("fuse/"+v.Name, func(t *testing.T) {
			out, err := k.fuse(ctx, []byte(v.Ancestor), []byte(v.A), []byte(v.B), []byte(v.PolicyDoc), []byte(v.Ctx))
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}

	for _, v := range vs.TransformFromCif {
		t.Run("transformFromCif/"+v.Name, func(t *testing.T) {
			out, err := k.TransformFromCIF(ctx, []byte(v.Cif), []byte(v.Schema), v.FormatID)
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}

	for _, v := range vs.Resolve {
		t.Run("resolve/"+v.Name, func(t *testing.T) {
			out, err := k.Resolve(ctx, []byte(v.Ancestor), []byte(v.Changelog), []byte(v.PolicyDoc), []byte(v.Ctx))
			if v.IsErr {
				if err == nil {
					t.Fatalf("want error, got %s", out)
				}
				if err.Error() != v.Expected {
					t.Errorf("got error %q want %q", err.Error(), v.Expected)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if string(out) != v.Expected {
				t.Errorf("got %s want %s", out, v.Expected)
			}
		})
	}
}
