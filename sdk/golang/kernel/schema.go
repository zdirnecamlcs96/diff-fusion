package kernel

import (
	"encoding/json"
	"fmt"
	"reflect"
	"strings"
	"time"
)

// timeType is compared against by nodeFor to reject time.Time fields before
// generic struct recursion would silently derive {"type":"object"} for them.
var timeType = reflect.TypeOf(time.Time{})

// marshalerType is compared against by nodeFor to reject types with a custom
// MarshalJSON: reflection can't see the shape a custom Marshaler actually
// produces (e.g. a [16]byte UUID that marshals to a string).
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

// errEmptyObjectSchema is returned when a cif-tagged struct (as a plain
// field or as an array element) has no cif-tagged fields of its own: an
// opaque {"type":"object"} with no children/schema isn't allowed.
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

// SchemaFromStruct derives the CIF schema JSON that TransformToCIF and
// TransformFromCIF expect from v, the caller's own native struct — the one
// they already json.Marshal/Unmarshal for their system. format is the
// format_id under "transformations" that TransformToCIF/TransformFromCIF
// select by; it must be non-empty and cannot be "cif" (reserved for the CIF
// document itself). v is a struct value or pointer to one.
//
// Derivation rules, walking exported fields in declaration order (depth
// capped at 32 levels):
//
//   - Source path: a field's `json` tag name (text before the first comma),
//     or the exact Go field name if there's no `json` tag — the kernel
//     matches keys exactly, no case folding. `json:"-"` skips the field
//     entirely (nothing to read). `.`/`\` in a source key are escaped the
//     same way core/src/domain/json_path.rs does, so a literal dot in a
//     JSON key survives path splitting.
//   - `cif:"<name>"` / `cif:"<name>,required"` says the field belongs in
//     the CIF document as field <name>; `cif:"-"` skips the field (and, for
//     a struct, its whole subtree). No cif tag at all:
//   - on a scalar or a slice: the field is local-only and is skipped
//     (an array can't be flattened, so a cif-less slice is skipped too).
//   - on a struct or *struct: the field is transparent — it does not
//     become a CIF node itself, but its own fields are walked as if they
//     were declared directly on the parent, with their source paths
//     prefixed by this field's own (escaped) source key.
//   - A cif-tagged struct/*struct field becomes a CIF object; a cif-tagged
//     slice of struct becomes a CIF array. Either way its source_path is
//     this field's own (prefixed) source key, and its children/elements are
//     walked with a *fresh* relative path scope — matching the kernel's
//     source_path semantics, where children/element paths resolve against
//     the value already extracted at the parent's source_path.
//   - Nested objects must be a struct with cif-tagged fields — map fields
//     are rejected, and a struct with zero cif-tagged fields is rejected.
//   - Slice/array elements must be primitive scalars or cif-tagged structs.
//   - time.Time fields are rejected (declare as string, UTC RFC 3339).
//   - Types implementing json.Marshaler are rejected — reflection can't see
//     their custom JSON shape.
//   - Duplicate cif field names within the same CIF scope are rejected
//     (including two different native fields landing on the same name via
//     transparency).
//   - Embedded structs without a json tag are promoted like encoding/json
//     does: their fields are walked with no extra path segment.
func SchemaFromStruct(v any, format string) ([]byte, error) {
	if format == "" {
		return nil, fmt.Errorf("kernel: SchemaFromStruct: format must not be empty")
	}
	if format == "cif" {
		return nil, fmt.Errorf(`kernel: SchemaFromStruct: "cif" is reserved and cannot be used as a format name`)
	}

	t := reflect.TypeOf(v)
	for t != nil && t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t == nil || t.Kind() != reflect.Struct {
		return nil, fmt.Errorf("kernel: SchemaFromStruct: v must be a struct or pointer to struct, got %T", v)
	}

	cifSchema, rules, err := walkStruct(t, 0, "")
	if err != nil {
		return nil, err
	}
	if len(cifSchema) == 0 {
		return nil, fmt.Errorf("kernel: SchemaFromStruct: %w", errEmptyObjectSchema)
	}

	doc := schemaDoc{
		CifSchema:       cifSchema,
		Transformations: map[string]map[string]transformRule{format: rules},
	}
	return json.Marshal(doc)
}

// escapeSegment mirrors core/src/domain/json_path.rs escape_segment
// byte-for-byte: a literal `\` or `.` inside a JSON key must survive path
// splitting as part of the key, not be read as an escape/separator.
func escapeSegment(s string) string {
	s = strings.ReplaceAll(s, `\`, `\\`)
	return strings.ReplaceAll(s, ".", `\.`)
}

// sourceKeyFor returns f's source key — its `json` tag name (text before
// the first comma), defaulting to the exact Go field name when there's no
// `json` tag — and whether f should be skipped entirely because its json
// tag is exactly "-".
func sourceKeyFor(f reflect.StructField) (key string, skip bool) {
	tag, present := f.Tag.Lookup("json")
	if !present {
		return f.Name, false
	}
	if tag == "-" {
		return "", true
	}
	name, _, _ := strings.Cut(tag, ",")
	if name == "" {
		name = f.Name
	}
	return name, false
}

// cifTagState is the shape of a field's `cif` tag: absent (a struct field is
// transparent, anything else is a skipped local-only leaf), dash (skip the
// field, and for a struct its whole subtree, unconditionally), or named (the
// field becomes a CIF node).
type cifTagState int

const (
	cifAbsent cifTagState = iota
	cifDash
	cifNamed
)

// parseCifTag reads the `cif` tag off a struct field. err is non-nil when an
// option other than exactly "required" is present (e.g. a typo).
func parseCifTag(f reflect.StructField) (state cifTagState, name string, required bool, err error) {
	tag, present := f.Tag.Lookup("cif")
	if !present {
		return cifAbsent, "", false, nil
	}
	if tag == "-" {
		return cifDash, "", false, nil
	}
	parts := strings.Split(tag, ",")
	if parts[0] == "" {
		// Present but no name (e.g. `cif:",required"`): treat like an
		// explicit "-" rather than transparent flattening, since the tag
		// was deliberately set.
		return cifDash, "", false, nil
	}
	for _, opt := range parts[1:] {
		if opt != "required" {
			return cifNamed, "", false, fmt.Errorf("invalid cif tag option %q (only \"required\" is allowed)", opt)
		}
		required = true
	}
	return cifNamed, parts[0], required, nil
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
// into walkStruct — i.e. not time.Time and not a custom json.Marshaler,
// both of which need their own dedicated rejection message (or, for a
// cif-less field, plain skipping) instead of generic struct recursion.
func isCifTaggableStruct(t reflect.Type) bool {
	return t.Kind() == reflect.Struct && t != timeType &&
		!t.Implements(marshalerType) && !reflect.PointerTo(t).Implements(marshalerType)
}

// walkStruct walks t's exported fields, building both the "cif_schema" and
// the "transformations"-for-one-format map in lockstep (they always share
// the same node structure). prefix is the escaped, dot-joined, trailing-dot
// source path of any transparent ancestor fields already walked into this
// scope; "" at the root or whenever a CIF object/array's own children are
// entered (a fresh relative scope, matching the kernel's source_path
// semantics).
func walkStruct(t reflect.Type, depth int, prefix string) (map[string]cifFieldDef, map[string]transformRule, error) {
	if depth > maxSchemaDepth {
		return nil, nil, errSchemaTooDeep
	}
	cifFields := make(map[string]cifFieldDef)
	rules := make(map[string]transformRule)
	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		if f.PkgPath != "" { // unexported
			continue
		}
		key, skipJSON := sourceKeyFor(f)
		if skipJSON {
			continue
		}
		state, name, required, err := parseCifTag(f)
		if err != nil {
			return nil, nil, fmt.Errorf("kernel: field %s: %w", f.Name, err)
		}
		if state == cifDash {
			continue
		}

		underlying := f.Type
		for underlying.Kind() == reflect.Pointer {
			underlying = underlying.Elem()
		}

		if state == cifAbsent {
			if !isCifTaggableStruct(underlying) {
				continue // scalar, slice, map, etc: local-only, skip silently
			}
			childPrefix := prefix + escapeSegment(key) + "."
			if _, hasJSONTag := f.Tag.Lookup("json"); f.Anonymous && !hasJSONTag {
				// encoding/json promotes an embedded struct's fields to the
				// parent object with no key prefix when it has no json tag.
				childPrefix = prefix
			}
			childCif, childRules, err := walkStruct(underlying, depth+1, childPrefix)
			if err != nil {
				return nil, nil, fmt.Errorf("kernel: field %s: %w", f.Name, err)
			}
			for n, d := range childCif {
				if _, exists := cifFields[n]; exists {
					return nil, nil, fmt.Errorf("kernel: field %s: duplicate cif field name %q", f.Name, n)
				}
				cifFields[n] = d
				rules[n] = childRules[n]
			}
			continue
		}

		def, rule, err := nodeFor(f.Type, depth, prefix+escapeSegment(key))
		if err != nil {
			return nil, nil, fmt.Errorf("kernel: field %s: %w", f.Name, err)
		}
		def.Required = required
		if _, exists := cifFields[name]; exists {
			return nil, nil, fmt.Errorf("kernel: field %s: duplicate cif field name %q", f.Name, name)
		}
		cifFields[name] = def
		rules[name] = rule
	}
	return cifFields, rules, nil
}

// nodeFor classifies t — a cif-tagged field's Go type — and builds its
// schema/transform-rule pair. sourcePath is the escaped, prefixed source
// path for this node, used verbatim for scalars and scalar arrays; a
// struct/array-of-struct's own children are walked with a fresh ""
// prefix (relative to the value already extracted at sourcePath).
func nodeFor(t reflect.Type, depth int, sourcePath string) (cifFieldDef, transformRule, error) {
	if depth > maxSchemaDepth {
		return cifFieldDef{}, transformRule{}, errSchemaTooDeep
	}
	for t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t == timeType {
		return cifFieldDef{}, transformRule{}, errUnsupportedTime
	}
	if t.Implements(marshalerType) || reflect.PointerTo(t).Implements(marshalerType) {
		return cifFieldDef{}, transformRule{}, errUnsupportedMarshaler
	}
	if typ, ok := scalarCifType(t.Kind()); ok {
		return cifFieldDef{Type: typ}, transformRule{SourcePath: sourcePath, Type: typ}, nil
	}
	switch t.Kind() {
	case reflect.Struct:
		children, childRules, err := walkStruct(t, depth+1, "")
		if err != nil {
			return cifFieldDef{}, transformRule{}, err
		}
		if len(children) == 0 {
			return cifFieldDef{}, transformRule{}, errEmptyObjectSchema
		}
		return cifFieldDef{Type: "object", Children: children},
			transformRule{SourcePath: sourcePath, Type: "object", Children: childRules}, nil
	case reflect.Map:
		return cifFieldDef{}, transformRule{}, errUnsupportedMap
	case reflect.Slice, reflect.Array:
		elemT := t.Elem()
		for elemT.Kind() == reflect.Pointer {
			elemT = elemT.Elem()
		}
		if elemT == timeType {
			return cifFieldDef{}, transformRule{}, errUnsupportedTime
		}
		if elemT.Implements(marshalerType) || reflect.PointerTo(elemT).Implements(marshalerType) {
			return cifFieldDef{}, transformRule{}, errUnsupportedMarshaler
		}
		if _, ok := scalarCifType(elemT.Kind()); ok {
			// Scalar element: opaque array passthrough, matching the kernel's
			// no-"element" fallback (transform.rs has no per-element type shape
			// for non-object elements).
			return cifFieldDef{Type: "array"}, transformRule{SourcePath: sourcePath, Type: "array"}, nil
		}
		if isCifTaggableStruct(elemT) {
			elemFields, elemRules, err := walkStruct(elemT, depth+1, "")
			if err != nil {
				return cifFieldDef{}, transformRule{}, err
			}
			if len(elemFields) == 0 {
				return cifFieldDef{}, transformRule{}, errEmptyObjectSchema
			}
			return cifFieldDef{Type: "array", Element: elemFields},
				transformRule{SourcePath: sourcePath, Type: "array", Element: elemRules}, nil
		}
		return cifFieldDef{}, transformRule{}, fmt.Errorf("array element type %s is not supported: array elements must be primitive scalars or cif-tagged structs", elemT.Kind())
	default:
		return cifFieldDef{}, transformRule{}, fmt.Errorf("unsupported field kind %s", t.Kind())
	}
}
