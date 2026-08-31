package kernel

import (
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestSchemaFromStruct(t *testing.T) {
	type Dimensions struct {
		Width  float64 `cif:"width" a:"w"`
		Height float64 `cif:"height" a:"h"`
	}
	type Item struct {
		SKU        string     `cif:"sku,required" a:"sku"`
		Note       string     `cif:"note" b:"note"` // present for a's cif_schema but no source_path under "a"
		unexported string     `cif:"unexported"`    // unexported: must be skipped even with a cif tag
		Dimensions Dimensions `cif:"dimensions" a:"dims"`
	}
	type Doc struct {
		Items []Item `cif:"items" a:"lines"`
		Skip  string // no cif tag: omitted entirely
	}

	got, err := SchemaFromStruct(Doc{}, "a")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}

	want := `{
		"cif_schema": {
			"items": {
				"type": "array",
				"element": {
					"sku": {"type": "string", "required": true},
					"note": {"type": "string"},
					"dimensions": {
						"type": "object",
						"children": {
							"width": {"type": "number"},
							"height": {"type": "number"}
						}
					}
				}
			}
		},
		"transformations": {
			"a": {
				"items": {
					"source_path": "lines",
					"type": "array",
					"element": {
						"sku": {"source_path": "sku", "type": "string"},
						"dimensions": {
							"source_path": "dims",
							"type": "object",
							"children": {
								"width": {"source_path": "w", "type": "number"},
								"height": {"source_path": "h", "type": "number"}
							}
						}
					}
				}
			}
		}
	}`

	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructPointerInput(t *testing.T) {
	type Doc struct {
		Name string `cif:"name,required" a:"n"`
	}
	got, err := SchemaFromStruct(&Doc{}, "a")
	if err != nil {
		t.Fatalf("SchemaFromStruct: %v", err)
	}
	want := `{"cif_schema":{"name":{"type":"string","required":true}},"transformations":{"a":{"name":{"source_path":"n","type":"string"}}}}`
	assertJSONEqual(t, got, []byte(want))
}

func TestSchemaFromStructNonStructInput(t *testing.T) {
	_, err := SchemaFromStruct("not a struct")
	if err == nil {
		t.Fatal("want error for non-struct input")
	}
}

func TestSchemaFromStructUnsupportedFieldKind(t *testing.T) {
	type Bad struct {
		Fn func() `cif:"fn"`
	}
	_, err := SchemaFromStruct(Bad{})
	if err == nil {
		t.Fatal("want error for unsupported field kind")
	}
}

func TestSchemaFromStructUnsupportedTimeField(t *testing.T) {
	type Bad struct {
		At time.Time `cif:"at"`
	}
	_, err := SchemaFromStruct(Bad{})
	if err == nil {
		t.Fatal("want error for time.Time field")
	}
	if !strings.Contains(err.Error(), "UTC") {
		t.Fatalf("want error mentioning UTC, got: %v", err)
	}

	type BadPtr struct {
		At *time.Time `cif:"at"`
	}
	_, err = SchemaFromStruct(BadPtr{})
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
	_, err := SchemaFromStruct(Bad{})
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
	_, err := SchemaFromStruct(Doc{})
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
	_, err := SchemaFromStruct(Bad{})
	if err == nil {
		t.Fatal("want error for json.Marshaler field")
	}
	if !strings.Contains(err.Error(), "json.Marshaler") {
		t.Fatalf("want error about json.Marshaler, got: %v", err)
	}
}

func TestSchemaFromStructDuplicateFieldName(t *testing.T) {
	type Bad struct {
		A string `cif:"same"`
		B string `cif:"same"`
	}
	_, err := SchemaFromStruct(Bad{})
	if err == nil {
		t.Fatal("want error for duplicate cif field name")
	}
	if !strings.Contains(err.Error(), `duplicate cif field name "same"`) {
		t.Fatalf("want error about duplicate cif field name, got: %v", err)
	}
}

func TestSchemaFromStructEmptyArrayElement(t *testing.T) {
	type Empty struct {
		Untagged string // no cif tag
	}
	type Doc struct {
		Items []Empty `cif:"items"`
	}
	_, err := SchemaFromStruct(Doc{})
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
	_, err := SchemaFromStruct(Doc{})
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
	_, err := SchemaFromStruct(Node{})
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
	_, err := SchemaFromStruct(Doc{})
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
	_, err := SchemaFromStruct(Bad{})
	if err == nil {
		t.Fatal("want error for misspelled cif tag option")
	}
	if !strings.Contains(err.Error(), "invalid cif tag option") {
		t.Fatalf("want error about invalid cif tag option, got: %v", err)
	}

	type BadSpace struct {
		Name string `cif:"name, required"` // leading space before "required"
	}
	_, err = SchemaFromStruct(BadSpace{})
	if err == nil {
		t.Fatal("want error for cif tag option with leading space")
	}
	if !strings.Contains(err.Error(), "invalid cif tag option") {
		t.Fatalf("want error about invalid cif tag option, got: %v", err)
	}
}

func TestSchemaFromStructReservedFormatName(t *testing.T) {
	type Doc struct {
		Name string `cif:"name" x:"n"`
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
	_, err := SchemaFromStruct(Doc{})
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
// tagged Go struct produces the exact same TransformToCIF output as the
// hand-written vector schema, for the
// "array-of-objects-with-nested-children-composing" transformToCif vector
// (spec/vectors/kernel-vectors.json): array-of-objects elements composing
// with a nested object, plus an object composing a nested array of objects.
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

	// Reproduces the vector's schema:
	//   items:    array<{sku: string, dimensions: {width, height: number}}>
	//   supplier: object{name: string, addresses: array<{city: string}>}
	type Dimensions struct {
		Width  float64 `cif:"width" f:"w"`
		Height float64 `cif:"height" f:"h"`
	}
	type Item struct {
		SKU        string     `cif:"sku" f:"sku"`
		Dimensions Dimensions `cif:"dimensions" f:"dims"`
	}
	type Address struct {
		City string `cif:"city" f:"city"`
	}
	type Supplier struct {
		Name      string    `cif:"name" f:"name"`
		Addresses []Address `cif:"addresses" f:"addrs"`
	}
	type Doc struct {
		Items    []Item   `cif:"items" f:"lines"`
		Supplier Supplier `cif:"supplier" f:"vendor"`
	}

	derived, err := SchemaFromStruct(Doc{}, v.FormatID)
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
