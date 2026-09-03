package kernel

import (
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"
)

// TestSchemaFromStructHubspotGolden mirrors the sdk/golang/examples/schema
// hubspot example: a native struct with a transparent (untagged) nested
// struct, a cif-tagged nested struct, a cif-tagged slice of struct, a
// required leaf, and a local-only (no cif tag) leaf.
func TestSchemaFromStructHubspotGolden(t *testing.T) {
	type Address struct {
		City string `json:"city" cif:"city"`
		Zip  string `json:"zip" cif:"zip"`
	}
	type Tag struct {
		Name string `json:"name" cif:"label"`
	}
	type HubspotContact struct {
		Properties struct {
			Email   string  `json:"email" cif:"email,required"`
			Phone   string  `json:"phone" cif:"phone"`
			Address Address `json:"address" cif:"address"`
		} `json:"properties"` // transparent: no cif tag
		Tags    []Tag `json:"tags" cif:"tags"`
		HsScore int   `json:"hs_score"` // no cif tag: local-only, skipped
	}

	got, err := SchemaFromStruct(HubspotContact{}, "hubspot")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}

	want := `{
		"cif_schema": {
			"email": {"type": "string", "required": true},
			"phone": {"type": "string"},
			"address": {
				"type": "object",
				"children": {
					"city": {"type": "string"},
					"zip": {"type": "string"}
				}
			},
			"tags": {
				"type": "array",
				"element": {
					"label": {"type": "string"}
				}
			}
		},
		"transformations": {
			"hubspot": {
				"email": {"source_path": "properties.email", "type": "string"},
				"phone": {"source_path": "properties.phone", "type": "string"},
				"address": {
					"source_path": "properties.address",
					"type": "object",
					"children": {
						"city": {"source_path": "city", "type": "string"},
						"zip": {"source_path": "zip", "type": "string"}
					}
				},
				"tags": {
					"source_path": "tags",
					"type": "array",
					"element": {
						"label": {"source_path": "name", "type": "string"}
					}
				}
			}
		}
	}`

	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructTransparentStruct(t *testing.T) {
	type Doc struct {
		Properties struct {
			Name string `json:"name" cif:"name"`
		} `json:"properties"` // no cif tag: transparent
	}
	got, err := SchemaFromStruct(Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string"}},"transformations":{"f":{"name":{"source_path":"properties.name","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

// TestSchemaFromStructEmbeddedPromoted asserts an embedded struct with no
// json tag is promoted like encoding/json does: its fields land at the
// parent's own path, with no "Base." prefix. An embedded struct WITH a json
// tag is a named key, so it still gets the usual transparent-field prefix.
func TestSchemaFromStructEmbeddedPromoted(t *testing.T) {
	type Base struct {
		ID string `json:"id" cif:"id,required"`
	}
	type Rec struct {
		Base
		Name string `json:"name" cif:"name"`
	}
	got, err := SchemaFromStruct(Rec{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{
		"cif_schema": {
			"id": {"type": "string", "required": true},
			"name": {"type": "string"}
		},
		"transformations": {
			"f": {
				"id": {"source_path": "id", "type": "string"},
				"name": {"source_path": "name", "type": "string"}
			}
		}
	}`
	assertJSONEqual(t, got, []byte(want))

	type RecTagged struct {
		Base `json:"base"`
		Name string `json:"name" cif:"name"`
	}
	got, err = SchemaFromStruct(RecTagged{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want = `{
		"cif_schema": {
			"id": {"type": "string", "required": true},
			"name": {"type": "string"}
		},
		"transformations": {
			"f": {
				"id": {"source_path": "base.id", "type": "string"},
				"name": {"source_path": "name", "type": "string"}
			}
		}
	}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructJSONDashSkipsField(t *testing.T) {
	type Doc struct {
		Name string `json:"-" cif:"name"` // json:"-": skipped entirely, even though cif-tagged
		Kept string `json:"kept" cif:"kept"`
	}
	got, err := SchemaFromStruct(Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"kept":{"type":"string"}},"transformations":{"f":{"kept":{"source_path":"kept","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructNoJSONTagUsesFieldName(t *testing.T) {
	type Doc struct {
		Name string `cif:"name"` // no json tag: source key is the exact Go field name
	}
	got, err := SchemaFromStruct(Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string"}},"transformations":{"f":{"name":{"source_path":"Name","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructEscapesDottedJSONKey(t *testing.T) {
	type Doc struct {
		Name string `json:"a.b" cif:"name"`
	}
	got, err := SchemaFromStruct(Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string"}},"transformations":{"f":{"name":{"source_path":"a\\.b","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructRequiredFlag(t *testing.T) {
	type Doc struct {
		Name string `json:"name" cif:"name,required"`
	}
	got, err := SchemaFromStruct(Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string","required":true}},"transformations":{"f":{"name":{"source_path":"name","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructUntaggedLeafSkipped(t *testing.T) {
	type Doc struct {
		Name string `json:"name" cif:"name"`
		Skip string `json:"skip"` // no cif tag: local-only, skipped
	}
	got, err := SchemaFromStruct(Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string"}},"transformations":{"f":{"name":{"source_path":"name","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructUntaggedSliceSkipped(t *testing.T) {
	type Item struct {
		X string `json:"x" cif:"x"`
	}
	type Doc struct {
		Name  string `json:"name" cif:"name"`
		Items []Item `json:"items"` // no cif tag: an array can't be flattened, skipped
	}
	got, err := SchemaFromStruct(Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string"}},"transformations":{"f":{"name":{"source_path":"name","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructDuplicateFieldName(t *testing.T) {
	type Bad struct {
		A string `json:"a" cif:"same"`
		B string `json:"b" cif:"same"`
	}
	_, err := SchemaFromStruct(Bad{}, "f")
	if err == nil {
		t.Fatal("want error for duplicate cif field name")
	}
	if !strings.Contains(err.Error(), `duplicate cif field name "same"`) {
		t.Fatalf("want error about duplicate cif field name, got: %v", err)
	}
}

func TestSchemaFromStructDuplicateFieldNameViaTransparency(t *testing.T) {
	type Inner struct {
		X string `json:"x" cif:"same"`
	}
	type Doc struct {
		A Inner  `json:"a"` // transparent
		B string `json:"b" cif:"same"`
	}
	_, err := SchemaFromStruct(Doc{}, "f")
	if err == nil {
		t.Fatal("want error for duplicate cif field name via transparency")
	}
	if !strings.Contains(err.Error(), `duplicate cif field name "same"`) {
		t.Fatalf("want error about duplicate cif field name, got: %v", err)
	}
}

func TestSchemaFromStructPointerInput(t *testing.T) {
	type Doc struct {
		Name string `json:"name" cif:"name,required"`
	}
	got, err := SchemaFromStruct(&Doc{}, "f")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string","required":true}},"transformations":{"f":{"name":{"source_path":"name","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructNonStructInput(t *testing.T) {
	_, err := SchemaFromStruct("not a struct", "f")
	if err == nil {
		t.Fatal("want error for non-struct input")
	}
}

func TestSchemaFromStructUnsupportedFieldKind(t *testing.T) {
	type Bad struct {
		Fn func() `cif:"fn"`
	}
	_, err := SchemaFromStruct(Bad{}, "f")
	if err == nil {
		t.Fatal("want error for unsupported field kind")
	}
}

func TestSchemaFromStructUnsupportedTimeField(t *testing.T) {
	type Bad struct {
		At time.Time `cif:"at"`
	}
	_, err := SchemaFromStruct(Bad{}, "f")
	if err == nil {
		t.Fatal("want error for time.Time field")
	}
	if !strings.Contains(err.Error(), "UTC") {
		t.Fatalf("want error mentioning UTC, got: %v", err)
	}

	type BadPtr struct {
		At *time.Time `cif:"at"`
	}
	_, err = SchemaFromStruct(BadPtr{}, "f")
	if err == nil {
		t.Fatal("want error for *time.Time field")
	}
	if !strings.Contains(err.Error(), "UTC") {
		t.Fatalf("want error mentioning UTC, got: %v", err)
	}
}

func TestSchemaFromStructUnsupportedMapField(t *testing.T) {
	type Bad struct {
		Attrs map[string]string `cif:"attrs"`
	}
	_, err := SchemaFromStruct(Bad{}, "f")
	if err == nil {
		t.Fatal("want error for map field")
	}
	if !strings.Contains(err.Error(), "map fields are not supported") {
		t.Fatalf("want error about unsupported map fields, got: %v", err)
	}
}

func TestSchemaFromStructEmptyNestedObject(t *testing.T) {
	type Empty struct {
		Untagged string // no cif tag
	}
	type Doc struct {
		Nested Empty `cif:"nested"`
	}
	_, err := SchemaFromStruct(Doc{}, "f")
	if err == nil {
		t.Fatal("want error for nested struct with no cif-tagged fields")
	}
	if !strings.Contains(err.Error(), "no cif-tagged fields") {
		t.Fatalf("want error about no cif-tagged fields, got: %v", err)
	}
}

// marshalerID implements json.Marshaler with a shape reflection can't see
// (a [16]byte array that marshals to a string), for
// TestSchemaFromStructUnsupportedMarshalerField.
type marshalerID [16]byte

func (marshalerID) MarshalJSON() ([]byte, error) { return []byte(`"marshaled-id"`), nil }

func TestSchemaFromStructUnsupportedMarshalerField(t *testing.T) {
	type Bad struct {
		ID marshalerID `cif:"id"`
	}
	_, err := SchemaFromStruct(Bad{}, "f")
	if err == nil {
		t.Fatal("want error for json.Marshaler field")
	}
	if !strings.Contains(err.Error(), "json.Marshaler") {
		t.Fatalf("want error about json.Marshaler, got: %v", err)
	}
}

func TestSchemaFromStructEmptyArrayElement(t *testing.T) {
	type Empty struct {
		Untagged string // no cif tag
	}
	type Doc struct {
		Items []Empty `cif:"items"`
	}
	_, err := SchemaFromStruct(Doc{}, "f")
	if err == nil {
		t.Fatal("want error for array element struct with no cif-tagged fields")
	}
	if !strings.Contains(err.Error(), "no cif-tagged fields") {
		t.Fatalf("want error about no cif-tagged fields, got: %v", err)
	}
}

func TestSchemaFromStructUnsupportedArrayElementKind(t *testing.T) {
	type Doc struct {
		Grid [][]string `cif:"grid"`
	}
	_, err := SchemaFromStruct(Doc{}, "f")
	if err == nil {
		t.Fatal("want error for array of array element")
	}
	if !strings.Contains(err.Error(), "array element type") {
		t.Fatalf("want error about unsupported array element type, got: %v", err)
	}
}

func TestSchemaFromStructRecursiveType(t *testing.T) {
	type Node struct {
		Next *Node `cif:"next"`
	}
	_, err := SchemaFromStruct(Node{}, "f")
	if err == nil {
		t.Fatal("want error for recursive type")
	}
	if !strings.Contains(err.Error(), "recursive types are not supported") {
		t.Fatalf("want error about recursion depth, got: %v", err)
	}
}

func TestSchemaFromStructRootEmptySchema(t *testing.T) {
	type Doc struct {
		Untagged string // no cif tag
	}
	_, err := SchemaFromStruct(Doc{}, "f")
	if err == nil {
		t.Fatal("want error for root struct with no cif-tagged fields")
	}
	if !strings.Contains(err.Error(), "no cif-tagged fields") {
		t.Fatalf("want error about no cif-tagged fields, got: %v", err)
	}
}

func TestSchemaFromStructInvalidCifTagOption(t *testing.T) {
	type Bad struct {
		Name string `cif:"name,requierd"`
	}
	_, err := SchemaFromStruct(Bad{}, "f")
	if err == nil {
		t.Fatal("want error for misspelled cif tag option")
	}
	if !strings.Contains(err.Error(), "invalid cif tag option") {
		t.Fatalf("want error about invalid cif tag option, got: %v", err)
	}

	type BadSpace struct {
		Name string `cif:"name, required"` // leading space before "required"
	}
	_, err = SchemaFromStruct(BadSpace{}, "f")
	if err == nil {
		t.Fatal("want error for cif tag option with leading space")
	}
	if !strings.Contains(err.Error(), "invalid cif tag option") {
		t.Fatalf("want error about invalid cif tag option, got: %v", err)
	}
}

func TestSchemaFromStructEmptyFormatRejected(t *testing.T) {
	type Doc struct {
		Name string `json:"name" cif:"name"`
	}
	_, err := SchemaFromStruct(Doc{}, "")
	if err == nil {
		t.Fatal("want error for empty format")
	}
}

func TestSchemaFromStructReservedFormatName(t *testing.T) {
	type Doc struct {
		Name string `json:"name" cif:"name"`
	}
	_, err := SchemaFromStruct(Doc{}, "cif")
	if err == nil {
		t.Fatal(`want error for reserved format name "cif"`)
	}
	if !strings.Contains(err.Error(), "reserved") {
		t.Fatalf("want error about reserved format name, got: %v", err)
	}
}

// marshalerStatus implements json.Marshaler on top of a scalar kind
// (string), for TestSchemaFromStructUnsupportedMarshalerArrayElement: a
// slice of it must still be rejected, not slip through the array branch's
// scalar-kind fast path.
type marshalerStatus string

func (marshalerStatus) MarshalJSON() ([]byte, error) { return []byte(`"status"`), nil }

func TestSchemaFromStructUnsupportedMarshalerArrayElement(t *testing.T) {
	type Doc struct {
		Statuses []marshalerStatus `cif:"statuses"`
	}
	_, err := SchemaFromStruct(Doc{}, "f")
	if err == nil {
		t.Fatal("want error for array of json.Marshaler scalar type")
	}
	if !strings.Contains(err.Error(), "json.Marshaler") {
		t.Fatalf("want error about json.Marshaler, got: %v", err)
	}
}

func assertJSONEqual(t *testing.T, got, want []byte) {
	t.Helper()
	var gotVal, wantVal any
	if err := json.Unmarshal(got, &gotVal); err != nil {
		t.Fatalf("unmarshal got: %v\n%s", err, got)
	}
	if err := json.Unmarshal(want, &wantVal); err != nil {
		t.Fatalf("unmarshal want: %v\n%s", err, want)
	}
	if !reflect.DeepEqual(gotVal, wantVal) {
		t.Errorf("schema mismatch\ngot:  %s\nwant: %s", got, want)
	}
}

// TestSchemaFromStructParityWithVector proves a schema derived from a
// native Go struct produces the exact same TransformToCIF output as the
// hand-written vector schema, for the
// "array-of-objects-with-nested-children-composing" transformToCif vector
// (spec/vectors/kernel-vectors.json): array-of-objects elements composing
// with a nested object, plus an object composing a nested array of objects.
// The struct mirrors the vector's ENTITY shape (its json tags match the
// vector's source document keys) with cif tags naming the vector's CIF
// fields.
func TestSchemaFromStructParityWithVector(t *testing.T) {
	raw, err := os.ReadFile("../../../spec/vectors/kernel-vectors.json")
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var vs kernelVectors
	if err := json.Unmarshal(raw, &vs); err != nil {
		t.Fatalf("parse vectors: %v", err)
	}
	var v *transformToCifVector
	for i := range vs.TransformToCif {
		if vs.TransformToCif[i].Name == "array-of-objects-with-nested-children-composing" {
			v = &vs.TransformToCif[i]
			break
		}
	}
	if v == nil {
		t.Fatal("vector not found: array-of-objects-with-nested-children-composing")
	}

	// Reproduces the vector's source document / schema:
	//   {"lines": [{"sku": ..., "dims": {"w": ..., "h": ...}}], "vendor": {"name": ..., "addrs": [{"city": ...}]}}
	//   items:    array<{sku: string, dimensions: {width, height: number}}>
	//   supplier: object{name: string, addresses: array<{city: string}>}
	type Item struct {
		SKU        string `json:"sku" cif:"sku"`
		Dimensions struct {
			Width  float64 `json:"w" cif:"width"`
			Height float64 `json:"h" cif:"height"`
		} `json:"dims" cif:"dimensions"`
	}
	type Supplier struct {
		Name      string `json:"name" cif:"name"`
		Addresses []struct {
			City string `json:"city" cif:"city"`
		} `json:"addrs" cif:"addresses"`
	}
	type Entity struct {
		Items    []Item   `json:"lines" cif:"items"`
		Supplier Supplier `json:"vendor" cif:"supplier"`
	}

	derived, err := SchemaFromStruct(Entity{}, v.FormatID)
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	assertJSONEqual(t, derived, []byte(v.Schema))

	k, ctx := newKernel(t)

	wantOut, err := k.TransformToCIF(ctx, []byte(v.Source), []byte(v.Schema), v.FormatID)
	if err != nil {
		t.Fatalf("TransformToCIF (vector schema): %v", err)
	}
	gotOut, err := k.TransformToCIF(ctx, []byte(v.Source), derived, v.FormatID)
	if err != nil {
		t.Fatalf("TransformToCIF (derived schema): %v", err)
	}
	if string(gotOut) != string(wantOut) {
		t.Errorf("output mismatch\ngot:  %s\nwant: %s", gotOut, wantOut)
	}
	if string(gotOut) != v.Expected {
		t.Errorf("output mismatch vs vector.Expected\ngot:  %s\nwant: %s", gotOut, v.Expected)
	}
}
