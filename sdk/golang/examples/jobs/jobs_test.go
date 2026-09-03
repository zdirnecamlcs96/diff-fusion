package jobs

import (
	"encoding/json"
	"testing"
)

type Doc struct {
	SKU string  `cif:"sku,required" erp:"stock_code"`
	Qty float64 `cif:"qty"          erp:"quantity"`
}

type Entity struct {
	StockCode string  `json:"stock_code"`
	Quantity  float64 `json:"quantity"`
	Note      string  `json:"note"` // local-only: never crosses
}

func TestRoundTrip(t *testing.T) {
	ancestor := Entity{StockCode: "A1", Quantity: 3, Note: "local"}
	mine := ancestor
	mine.Quantity = 5
	theirs := ancestor
	theirs.StockCode = "A1-R"

	ancestorOut, err := TransformIn[Doc](TransformInput[Doc]{Format: "erp", Doc: ancestor})
	if err != nil {
		t.Fatalf("TransformIn(ancestor): %v", err)
	}
	mineOut, err := TransformIn[Doc](TransformInput[Doc]{Format: "erp", Doc: mine})
	if err != nil {
		t.Fatalf("TransformIn(mine): %v", err)
	}
	theirsOut, err := TransformIn[Doc](TransformInput[Doc]{Format: "erp", Doc: theirs})
	if err != nil {
		t.Fatalf("TransformIn(theirs): %v", err)
	}
	ancestorCIF := ancestorOut.Doc

	changelog, err := Detect(ancestorOut.Doc, mineOut.Doc, theirsOut.Doc)
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

	// out.Value is a json.RawMessage; TransformOut takes it directly, no conversion.
	patchOut, err := TransformOut[Doc](TransformInput[Doc]{Format: "erp", Doc: out.Value})
	if err != nil {
		t.Fatalf("TransformOut: %v", err)
	}
	patch := patchOut.Doc

	got := ancestor
	if err := json.Unmarshal(patch, &got); err != nil {
		t.Fatalf("apply patch %s: %v", patch, err)
	}
	want := Entity{StockCode: "A1-R", Quantity: 5, Note: "local"}
	if got != want {
		t.Fatalf("got %+v (patch=%s) want %+v", got, patch, want)
	}
}
