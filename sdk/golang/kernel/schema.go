package kernel

import (
	"encoding/json"
	"fmt"
	"reflect"
	"strings"
	"time"
)

// timeType is compared against by cifFieldDefFor and transformRuleFor to
// reject time.Time fields before generic struct recursion would silently
// derive {"type":"object"} for them.
var timeType = reflect.TypeOf(time.Time{})

// marshalerType is compared against by cifFieldDefFor and transformRuleFor
// to reject types with a custom MarshalJSON: reflection can't see the
// shape a custom Marshaler actually produces (e.g. a [16]byte UUID that
// marshals to a string).
var marshalerType = reflect.TypeOf((*json.Marshaler)(nil)).Elem()

// maxSchemaDepth caps struct/array recursion so a self-referential type
// (e.g. `type Node struct { Next *Node }`) errors instead of overflowing
// the stack.
const maxSchemaDepth = 32

// errUnsupportedTime is returned for time.Time fields: the kernel has no
// timestamp type, so callers must declare the field as a string holding a
// UTC RFC 3339 timestamp instead.
var errUnsupportedTime = fmt.Errorf("time.Time is not supported: declare the field as string holding a UTC RFC 3339 timestamp")

// errUnsupportedMarshaler is returned for types implementing json.Marshaler:
// reflection walks struct shape, not the custom MarshalJSON output.
var errUnsupportedMarshaler = fmt.Errorf("type implements json.Marshaler: reflection cannot see its JSON shape — declare the field with the marshaled type (e.g. string)")

// errUnsupportedMap is returned for map-typed fields: field names aren't
// known ahead of time, so the kernel schema has no way to describe them.
var errUnsupportedMap = fmt.Errorf("map fields are not supported: nested objects must declare a schema — use a struct with cif tags")

// errEmptyObjectSchema is returned when a nested struct (as a plain field
// or as an array element) has no cif-tagged fields: an opaque
// {"type":"object"} with no children/schema isn't allowed.
var errEmptyObjectSchema = fmt.Errorf("struct has no cif-tagged fields: nested objects must declare a schema")

// errSchemaTooDeep is returned once struct/array recursion exceeds
// maxSchemaDepth.
var errSchemaTooDeep = fmt.Errorf("schema nesting exceeds 32 levels: recursive types are not supported")

// cifFieldDef mirrors one entry of the kernel schema's "cif_schema" map
// (core/src/application/transform.rs). "required" is omitted when false to
// match the kernel-generated vectors byte-for-byte.
type cifFieldDef struct {
	Type     string                 `json:"type"`
	Required bool                   `json:"required,omitempty"`
	Element  map[string]cifFieldDef `json:"element,omitempty"`
	Children map[string]cifFieldDef `json:"children,omitempty"`
}

// transformRule mirrors one entry of a format's map under "transformations".
type transformRule struct {
	SourcePath string                   `json:"source_path"`
	Type       string                   `json:"type"`
	Element    map[string]transformRule `json:"element,omitempty"`
	Children   map[string]transformRule `json:"children,omitempty"`
}

type schemaDoc struct {
	CifSchema       map[string]cifFieldDef              `json:"cif_schema"`
	Transformations map[string]map[string]transformRule `json:"transformations"`
}

// SchemaFromStruct derives the CIF schema JSON from a tagged struct, so
// callers can generate schema.json from a Go type instead of hand-writing
// it. v is a struct value or pointer to one. formats lists the struct tag
// keys to read source paths from; each becomes a format_id under
// "transformations".
//
// Tag rules:
//   - `cif:"<field_name>"` names the CIF field; `cif:"<field_name>,required"`
//     marks it required. No cif tag, or `cif:"-"`, skips the field entirely.
//   - For each format F in formats, the struct tag key F holds the
//     source_path for that field under format F (relative to the enclosing
//     element/children scope for nested fields). A field missing tag F is
//     omitted from that format's transformations — per the CIF contract,
//     unmapped fields are local-only state, not an error.
func SchemaFromStruct(v any, formats ...string) ([]byte, error) {
	t := reflect.TypeOf(v)
	for t != nil && t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t == nil || t.Kind() != reflect.Struct {
		return nil, fmt.Errorf("kernel: SchemaFromStruct: v must be a struct or pointer to struct, got %T", v)
	}

	cifSchema, err := structCifFields(t, 0)
	if err != nil {
		return nil, err
	}
	if len(cifSchema) == 0 {
		return nil, fmt.Errorf("kernel: SchemaFromStruct: %w", errEmptyObjectSchema)
	}

	transformations := make(map[string]map[string]transformRule, len(formats))
	for _, format := range formats {
		if format == "cif" {
			return nil, fmt.Errorf(`kernel: SchemaFromStruct: "cif" is reserved and cannot be used as a format name`)
		}
		rules, err := structTransformRules(t, format, 0)
		if err != nil {
			return nil, err
		}
		transformations[format] = rules
	}

	return json.Marshal(schemaDoc{CifSchema: cifSchema, Transformations: transformations})
}

// parseCifTag reads the `cif` tag off a struct field: the CIF field name and
// whether "required" was requested. ok is false when the field has no cif
// tag, or `cif:"-"`, meaning "skip this field entirely". err is non-nil when
// an option other than exactly "required" is present (e.g. a typo).
func parseCifTag(f reflect.StructField) (name string, required bool, ok bool, err error) {
	tag, present := f.Tag.Lookup("cif")
	if !present || tag == "-" {
		return "", false, false, nil
	}
	parts := strings.Split(tag, ",")
	if parts[0] == "" {
		return "", false, false, nil
	}
	for _, opt := range parts[1:] {
		if opt != "required" {
			return "", false, false, fmt.Errorf("invalid cif tag option %q (only \"required\" is allowed)", opt)
		}
		required = true
	}
	return parts[0], required, true, nil
}

// scalarCifType maps a Go kind to the kernel's scalar type vocabulary
// (verified against transform.rs normalize_type: "string" | "number" |
// "boolean"). ok is false for kinds that aren't scalars.
func scalarCifType(k reflect.Kind) (string, bool) {
	switch k {
	case reflect.String:
		return "string", true
	case reflect.Bool:
		return "boolean", true
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64,
		reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr,
		reflect.Float32, reflect.Float64:
		return "number", true
	default:
		return "", false
	}
}

// isCifTaggableStruct reports whether t is a struct kind that can recurse
// into structCifFields/structTransformRules — i.e. not time.Time and not a
// custom json.Marshaler, both of which need their own dedicated rejection
// message instead of a generic struct recursion.
func isCifTaggableStruct(t reflect.Type) bool {
	return t.Kind() == reflect.Struct && t != timeType &&
		!t.Implements(marshalerType) && !reflect.PointerTo(t).Implements(marshalerType)
}

func structCifFields(t reflect.Type, depth int) (map[string]cifFieldDef, error) {
	out := make(map[string]cifFieldDef)
	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		if f.PkgPath != "" { // unexported
			continue
		}
		name, required, ok, err := parseCifTag(f)
		if err != nil {
			return nil, fmt.Errorf("kernel: field %s: %w", f.Name, err)
		}
		if !ok {
			continue
		}
		def, err := cifFieldDefFor(f.Type, depth)
		if err != nil {
			return nil, fmt.Errorf("kernel: field %s: %w", f.Name, err)
		}
		def.Required = required
		if _, exists := out[name]; exists {
			return nil, fmt.Errorf("kernel: field %s: duplicate cif field name %q", f.Name, name)
		}
		out[name] = def
	}
	return out, nil
}

func cifFieldDefFor(t reflect.Type, depth int) (cifFieldDef, error) {
	if depth > maxSchemaDepth {
		return cifFieldDef{}, errSchemaTooDeep
	}
	for t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t == timeType {
		return cifFieldDef{}, errUnsupportedTime
	}
	if t.Implements(marshalerType) || reflect.PointerTo(t).Implements(marshalerType) {
		return cifFieldDef{}, errUnsupportedMarshaler
	}
	if typ, ok := scalarCifType(t.Kind()); ok {
		return cifFieldDef{Type: typ}, nil
	}
	switch t.Kind() {
	case reflect.Struct:
		children, err := structCifFields(t, depth+1)
		if err != nil {
			return cifFieldDef{}, err
		}
		if len(children) == 0 {
			return cifFieldDef{}, errEmptyObjectSchema
		}
		return cifFieldDef{Type: "object", Children: children}, nil
	case reflect.Map:
		return cifFieldDef{}, errUnsupportedMap
	case reflect.Slice, reflect.Array:
		elemT := t.Elem()
		for elemT.Kind() == reflect.Pointer {
			elemT = elemT.Elem()
		}
		if elemT == timeType {
			return cifFieldDef{}, errUnsupportedTime
		}
		if elemT.Implements(marshalerType) || reflect.PointerTo(elemT).Implements(marshalerType) {
			return cifFieldDef{}, errUnsupportedMarshaler
		}
		if _, ok := scalarCifType(elemT.Kind()); ok {
			// Scalar element: opaque array passthrough, matching the kernel's
			// no-"element" fallback (transform.rs has no per-element type shape
			// for non-object elements).
			return cifFieldDef{Type: "array"}, nil
		}
		if isCifTaggableStruct(elemT) {
			elemFields, err := structCifFields(elemT, depth+1)
			if err != nil {
				return cifFieldDef{}, err
			}
			if len(elemFields) == 0 {
				return cifFieldDef{}, errEmptyObjectSchema
			}
			return cifFieldDef{Type: "array", Element: elemFields}, nil
		}
		return cifFieldDef{}, fmt.Errorf("array element type %s is not supported: array elements must be primitive scalars or cif-tagged structs", elemT.Kind())
	default:
		return cifFieldDef{}, fmt.Errorf("unsupported field kind %s", t.Kind())
	}
}

func structTransformRules(t reflect.Type, format string, depth int) (map[string]transformRule, error) {
	out := make(map[string]transformRule)
	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		if f.PkgPath != "" { // unexported
			continue
		}
		name, _, ok, err := parseCifTag(f)
		if err != nil {
			return nil, fmt.Errorf("kernel: field %s: %w", f.Name, err)
		}
		if !ok {
			continue
		}
		sourcePath, present := f.Tag.Lookup(format)
		if !present {
			continue // unmapped for this format: local-only, not an error
		}
		rule, err := transformRuleFor(f.Type, sourcePath, format, depth)
		if err != nil {
			return nil, fmt.Errorf("kernel: field %s: %w", f.Name, err)
		}
		if _, exists := out[name]; exists {
			return nil, fmt.Errorf("kernel: field %s: duplicate cif field name %q", f.Name, name)
		}
		out[name] = rule
	}
	return out, nil
}

func transformRuleFor(t reflect.Type, sourcePath, format string, depth int) (transformRule, error) {
	if depth > maxSchemaDepth {
		return transformRule{}, errSchemaTooDeep
	}
	for t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t == timeType {
		return transformRule{}, errUnsupportedTime
	}
	if t.Implements(marshalerType) || reflect.PointerTo(t).Implements(marshalerType) {
		return transformRule{}, errUnsupportedMarshaler
	}
	if typ, ok := scalarCifType(t.Kind()); ok {
		return transformRule{SourcePath: sourcePath, Type: typ}, nil
	}
	switch t.Kind() {
	case reflect.Struct:
		children, err := structTransformRules(t, format, depth+1)
		if err != nil {
			return transformRule{}, err
		}
		if len(children) == 0 {
			return transformRule{}, errEmptyObjectSchema
		}
		return transformRule{SourcePath: sourcePath, Type: "object", Children: children}, nil
	case reflect.Map:
		return transformRule{}, errUnsupportedMap
	case reflect.Slice, reflect.Array:
		elemT := t.Elem()
		for elemT.Kind() == reflect.Pointer {
			elemT = elemT.Elem()
		}
		if elemT == timeType {
			return transformRule{}, errUnsupportedTime
		}
		if elemT.Implements(marshalerType) || reflect.PointerTo(elemT).Implements(marshalerType) {
			return transformRule{}, errUnsupportedMarshaler
		}
		if _, ok := scalarCifType(elemT.Kind()); ok {
			return transformRule{SourcePath: sourcePath, Type: "array"}, nil
		}
		if isCifTaggableStruct(elemT) {
			elemRules, err := structTransformRules(elemT, format, depth+1)
			if err != nil {
				return transformRule{}, err
			}
			if len(elemRules) == 0 {
				return transformRule{}, errEmptyObjectSchema
			}
			return transformRule{SourcePath: sourcePath, Type: "array", Element: elemRules}, nil
		}
		return transformRule{}, fmt.Errorf("array element type %s is not supported: array elements must be primitive scalars or cif-tagged structs", elemT.Kind())
	default:
		return transformRule{}, fmt.Errorf("unsupported field kind %s", t.Kind())
	}
}
