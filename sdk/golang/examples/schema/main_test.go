package main

import (
	"encoding/json"
	"testing"

	"github.com/zdirnecamlcs96/diff-fusion/sdk/golang/kernel"
)

func TestHubspotContactSchema(t *testing.T) {
	schema, err := kernel.SchemaFromStruct(new(HubspotContact), "hubspot")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}

	var doc struct {
		CifSchema       map[string]any            `json:"cif_schema"`
		Transformations map[string]map[string]any `json:"transformations"`
	}
	if err := json.Unmarshal(schema, &doc); err != nil {
		t.Fatalf("unmarshal schema: %v", err)
	}

	for _, name := range []string{"email", "phone", "address", "tags"} {
		if _, ok := doc.CifSchema[name]; !ok {
			t.Errorf("cif_schema missing field %q", name)
		}
	}
	for _, name := range []string{"hs_score", "properties"} {
		if _, ok := doc.CifSchema[name]; ok {
			t.Errorf("cif_schema has unexpected field %q", name)
		}
	}

	if len(doc.Transformations) != 1 {
		t.Fatalf("transformations has %d formats, want 1 (hubspot): %v", len(doc.Transformations), doc.Transformations)
	}
	if _, ok := doc.Transformations["hubspot"]; !ok {
		t.Error("transformations missing hubspot")
	}
}
