#!/usr/bin/env python3
"""Render spec/schema/*.json (draft-07, schemars 0.8) into docs/reference
field-reference pages for the Jekyll site.

# ponytail: handles the constructs actually present in the four schemas
# today ($ref, array, map/additionalProperties, enum, a 2-branch Option<T>
# anyOf, bare `true` any-value properties, oneOf tagged unions, and a bare
# string-enum definition) and nothing more. No allOf, no >2-branch anyOf, no
# nested inline object schemas, no `$defs` (draft-2020-12) — only top-level
# draft-07 `definitions`. Add support when a schema actually needs it.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCHEMA_DIR = ROOT / "spec" / "schema"
OUT_DIR = ROOT / "docs" / "reference"

FRONT_MATTER = "---\nlayout: default\ntitle: {title}\nparent: Reference\nnav_order: {nav_order}\n---\n"


def ref_name(ref: str) -> str:
    return ref.rsplit("/", 1)[-1]


def anchor(name: str) -> str:
    # ponytail: matches kramdown's default auto-id (lowercase) for the
    # single-word CamelCase names these schemas actually use.
    return name.lower()


def type_str(schema) -> str:
    """Render a property/definition schema fragment as a markdown type string."""
    if isinstance(schema, bool):
        return "any"  # bare JSON Schema `true` — matches any value
    if "$ref" in schema:
        name = ref_name(schema["$ref"])
        return f"[{name}](#{anchor(name)})"
    if "enum" in schema:
        return " \\| ".join(f"`{v}`" for v in schema["enum"])
    if "anyOf" in schema:
        variants = [v for v in schema["anyOf"] if v.get("type") != "null"]
        if len(variants) == 1:
            return type_str(variants[0])
        return "any"
    t = schema.get("type")
    if t == "array":
        return f"{type_str(schema.get('items', {}))}[]"
    if t == "object" and "additionalProperties" in schema:
        return f"map<string, {type_str(schema['additionalProperties'])}>"
    return t or "any"


def render_table(properties: dict, required: list) -> list:
    lines = ["| Property | Type | Required | Description |", "|---|---|---|---|"]
    for name, sub in properties.items():
        req = "✓" if name in required else ""
        desc = sub.get("description", "") if isinstance(sub, dict) else ""
        lines.append(f"| `{name}` | {type_str(sub)} | {req} | {desc} |")
    return lines


def render_variants(variants: list) -> list:
    # ponytail: kind-discriminated oneOf variants (all usages today) get
    # their tag as the heading; a variant with no `kind` enum falls back to
    # its own `title`, else a positional "Variant N" heading.
    lines = []
    for i, variant in enumerate(variants, start=1):
        kind = (variant.get("properties", {}).get("kind", {}).get("enum") or [None])[0]
        heading = f"`{kind}`" if kind is not None else variant.get("title", f"Variant {i}")
        lines.append(f"### {heading}")
        lines.append("")
        if variant.get("description"):
            lines += [variant["description"], ""]
        lines += render_table(variant.get("properties", {}), variant.get("required", []))
        lines.append("")
    return lines


def render_body(schema: dict) -> list:
    if "oneOf" in schema:
        return render_variants(schema["oneOf"])
    if schema.get("type") == "string" and "enum" in schema and "properties" not in schema:
        # ponytail: bare string-enum definition (e.g. ChangeSource) — not one
        # of the two required structural cases, so a one-liner, not a table.
        return [f"Type: {type_str(schema)}", ""]
    return render_table(schema.get("properties", {}), schema.get("required", [])) + [""]


def render_def(name: str, schema: dict) -> list:
    lines = [f"## {name}", ""]
    if schema.get("description"):
        lines += [schema["description"], ""]
    lines += render_body(schema)
    return lines


def render_schema(schema: dict, nav_order: int) -> str:
    title = schema.get("title", "Untitled")
    out = [FRONT_MATTER.format(title=title, nav_order=nav_order), f"# {title}", ""]
    if schema.get("description"):
        out += [schema["description"], ""]
    out += render_body(schema)
    for def_name, def_schema in schema.get("definitions", {}).items():
        out += render_def(def_name, def_schema)
    return "\n".join(out) + "\n"


def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for i, path in enumerate(sorted(SCHEMA_DIR.glob("*.json")), start=5):
        schema = json.loads(path.read_text())
        md = render_schema(schema, i)
        out_path = OUT_DIR / (path.stem.replace(".schema", "") + ".md")
        out_path.write_text(md)
        print(f"wrote {out_path.relative_to(ROOT)}")


def selftest():
    assert type_str({"$ref": "#/definitions/Foo"}) == "[Foo](#foo)"
    assert type_str({"type": "array", "items": {"type": "string"}}) == "string[]"
    assert type_str({"type": "object", "additionalProperties": {"$ref": "#/definitions/Bar"}}) \
        == "map<string, [Bar](#bar)>"
    assert type_str({"enum": ["a", "b"]}) == "`a` \\| `b`"
    assert type_str({"type": "string"}) == "string"
    assert type_str({"anyOf": [{"$ref": "#/definitions/Baz"}, {"type": "null"}]}) == "[Baz](#baz)"
    assert type_str(True) == "any"
    assert render_variants([{"type": "string"}])[0] == "### Variant 1"
    print("selftest ok")


if __name__ == "__main__":
    import sys
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
