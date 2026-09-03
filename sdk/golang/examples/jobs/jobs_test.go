package jobs

import (
	"encoding/json"
	"testing"
)

type Entity struct {
	StockCode string  `json:"stock_code" cif:"sku,required"`
	Quantity  float64 `json:"quantity"   cif:"qty"`
	Note      string  `json:"note"` // no cif tag: local-only, never crosses
}

func TestRoundTrip(t *testing.T) {
	ancestor := Entity{StockCode: "A1", Quantity: 3, Note: "local"}
	mine := ancestor
	mine.Quantity = 5
	theirs := ancestor
	theirs.StockCode = "A1-R"

	ancestorCIF, err := TransformIn("erp", ancestor)
	if err != nil {
		t.Fatalf("TransformIn(ancestor): %v", err)
	}
	mineCIF, err := TransformIn("erp", mine)
	if err != nil {
		t.Fatalf("TransformIn(mine): %v", err)
	}
	theirsCIF, err := TransformIn("erp", theirs)
	if err != nil {
		t.Fatalf("TransformIn(theirs): %v", err)
	}

	changelog, err := Detect(ancestorCIF, mineCIF, theirsCIF)
	if err != nil {
		t.Fatalf("Detect: %v", err)
	}

	policy := []byte(`{"fields":{"qty":{"kind":"owned_by","system":"erp"},"sku":{"kind":"owned_by","system":"crm"}}}`)
	out, err := Resolve(ResolveInput{
		Ancestor:  ancestorCIF,
		Changelog: changelog,
		Policy:    policy,
		SystemA:   "erp",
		SystemB:   "crm",
	})
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if string(out.Conflicts) != "[]" {
		t.Fatalf("want no conflicts, got %s", out.Conflicts)
	}

	got, err := TransformOut("erp", out.Value, ancestor)
	if err != nil {
		t.Fatalf("TransformOut: %v", err)
	}
	want := Entity{StockCode: "A1-R", Quantity: 5, Note: "local"}
	if got != want {
		t.Fatalf("got %+v want %+v", got, want)
	}
}

// TestResolveOutputKeepsRawValue pins that embedding json.RawMessage promotes
// UnmarshalJSON: Resolve decodes the kernel's {"value":...} into a CIF unchanged.
func TestResolveOutputKeepsRawValue(t *testing.T) {
	var out ResolveOutput
	if err := json.Unmarshal([]byte(`{"value":{"qty":5,"sku":"A1"},"conflicts":[]}`), &out); err != nil {
		t.Fatal(err)
	}
	if string(out.Value.RawMessage) != `{"qty":5,"sku":"A1"}` || string(out.Conflicts) != `[]` {
		t.Fatalf("value %s conflicts %s", out.Value.RawMessage, out.Conflicts)
	}
}
