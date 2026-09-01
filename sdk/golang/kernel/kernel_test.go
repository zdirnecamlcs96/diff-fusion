package kernel

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"
)

type vector struct {
	CanonicalID          string          `json:"canonicalId"`
	CanonicalPayloadJSON string          `json:"canonicalPayloadJson"`
	ExpectedHex          string          `json:"expectedHex"`
	Operation            string          `json:"operation"`
	Payload              json.RawMessage `json:"payload"`
}

func newKernel(t *testing.T) (*Kernel, context.Context) {
	t.Helper()
	ctx := context.Background()
	k, err := New(ctx)
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	t.Cleanup(func() { k.Close(ctx) })
	return k, ctx
}

// The P4 gate: all 82 golden vectors, read straight from spec/vectors/
// (monorepo relative path — the Rust generator is the sole producer).
func TestGoldenIdempotencyVectors(t *testing.T) {
	raw, err := os.ReadFile("../../../spec/vectors/idempotency-vectors.json")
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var vs []vector
	if err := json.Unmarshal(raw, &vs); err != nil {
		t.Fatalf("parse vectors: %v", err)
	}
	if len(vs) != 82 {
		t.Fatalf("expected 82 vectors, got %d", len(vs))
	}
	k, ctx := newKernel(t)
	for i, v := range vs {
		canon, err := k.canonicalJSON(ctx, v.Payload)
		if err != nil {
			t.Fatalf("vector %d canonicalJSON: %v", i, err)
		}
		if string(canon) != v.CanonicalPayloadJSON {
			t.Errorf("vector %d canonical: got %s want %s", i, canon, v.CanonicalPayloadJSON)
		}
		hex, err := k.idempotencyKeyHex(ctx, v.CanonicalID, v.Operation, v.Payload)
		if err != nil {
			t.Fatalf("vector %d idempotencyKeyHex: %v", i, err)
		}
		if hex != v.ExpectedHex {
			t.Errorf("vector %d hex: got %s want %s", i, hex, v.ExpectedHex)
		}
	}
}

// Boundary smoke tests mirroring src/drivers/wire.rs tests.

func TestDiffNullClearIsPresentNullAndUntouchedSideOmitted(t *testing.T) {
	k, ctx := newKernel(t)
	out, err := k.threeWayDiff(ctx,
		[]byte(`{"status":"draft"}`),
		[]byte(`{"status":null}`),
		[]byte(`{"status":"draft"}`))
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if !strings.Contains(s, `"new_from_a":null`) {
		t.Errorf("want present-null new_from_a, got %s", s)
	}
	if strings.Contains(s, "new_from_b") {
		t.Errorf("want new_from_b absent, got %s", s)
	}
}

func TestMergeFieldAdditiveResolves(t *testing.T) {
	k, ctx := newKernel(t)
	out, err := k.mergeField(ctx,
		[]byte(`{"path":"qty","old_value":1,"new_from_a":3,"new_from_b":4,"source":"both"}`),
		[]byte(`{"kind":"additive"}`),
		[]byte(`{"system_a":"x","system_b":"y"}`))
	if err != nil {
		t.Fatal(err)
	}
	var res struct {
		Kind  string  `json:"kind"`
		Value float64 `json:"value"`
	}
	if err := json.Unmarshal(out, &res); err != nil {
		t.Fatal(err)
	}
	if res.Kind != "Resolved" || res.Value != 6.0 {
		t.Errorf("got %s", out)
	}
}

func TestInconsistentSourceIsError(t *testing.T) {
	k, ctx := newKernel(t)
	_, err := k.mergeField(ctx,
		[]byte(`{"path":"qty","old_value":1,"new_from_a":3,"source":"both"}`),
		[]byte(`{"kind":"additive"}`),
		[]byte(`{"system_a":"x","system_b":"y"}`))
	if err == nil || !strings.Contains(err.Error(), "inconsistent change") {
		t.Errorf("want inconsistent-change error, got %v", err)
	}
}

func TestBadJSONIsError(t *testing.T) {
	k, ctx := newKernel(t)
	if _, err := k.threeWayDiff(ctx, []byte(`{`), []byte(`{}`), []byte(`{}`)); err == nil {
		t.Error("want error for invalid JSON")
	}
}
