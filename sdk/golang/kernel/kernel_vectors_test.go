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

type kernelVectors struct {
	ThreeWayDiff   []threeWayVector       `json:"threeWayDiff"`
	MergeField     []mergeFieldVector     `json:"mergeField"`
	CompareJSON    []compareJSONVector    `json:"compareJson"`
	TransformToCif []transformToCifVector `json:"transformToCif"`
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
	if len(vs.ThreeWayDiff) != 16 || len(vs.MergeField) != 29 || len(vs.CompareJSON) != 8 || len(vs.TransformToCif) != 13 {
		t.Fatalf("expected 16 threeWayDiff + 29 mergeField + 8 compareJson + 13 transformToCif vectors, got %d + %d + %d + %d",
			len(vs.ThreeWayDiff), len(vs.MergeField), len(vs.CompareJSON), len(vs.TransformToCif))
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
			out, err := k.MergeField(ctx, []byte(v.Change), []byte(v.PolicyRef), []byte(v.Ctx))
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
			out, err := k.CompareJSON(ctx, []byte(v.A), []byte(v.B))
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
}
