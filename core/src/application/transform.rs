use crate::domain::json_path::split_path;
use serde_json::Value;
use std::error::Error;

/// Pure schema-driven conversion of source JSON into the Common
/// Intermediate Format (CIF). Zero state — every method is a pure function.
/// Internal namespace; the public entry point is [`transform_to_cif`].
pub(crate) struct Transformer;

impl Transformer {
    /// Transform a JSON document to CIF using the schema's `transformations`
    /// rules for `format_id`.
    pub fn to_cif(
        source: &Value,
        schema: &Value,
        format_id: &str,
    ) -> Result<Value, Box<dyn Error>> {
        let transformations = Self::get_transformations(schema, format_id)?;
        let cif_schema = Self::get_cif_schema(schema)?;

        let mut cif_object = serde_json::Map::new();
        if let Value::Object(cif_fields) = cif_schema {
            for (cif_field_name, cif_field_def) in cif_fields {
                Self::transform_field(
                    source,
                    transformations,
                    cif_field_name,
                    cif_field_def,
                    &mut cif_object,
                )?;
            }
        }

        Ok(Value::Object(cif_object))
    }

    fn get_transformations<'a>(
        schema: &'a Value,
        format_id: &str,
    ) -> Result<&'a Value, Box<dyn Error>> {
        schema
            .get("transformations")
            .and_then(|t| t.get(format_id))
            .ok_or_else(|| format!("Format '{format_id}' not found in schema").into())
    }

    fn get_cif_schema(schema: &Value) -> Result<&Value, Box<dyn Error>> {
        schema
            .get("cif_schema")
            .ok_or("'cif_schema' not defined in schema".into())
    }

    fn transform_field(
        source: &Value,
        transformations: &Value,
        cif_field_name: &str,
        cif_field_def: &Value,
        cif_object: &mut serde_json::Map<String, Value>,
    ) -> Result<(), Box<dyn Error>> {
        let Some(transform_rule) = transformations.get(cif_field_name) else {
            return Ok(());
        };

        let source_path = transform_rule
            .get("source_path")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("source_path not defined for field '{cif_field_name}'"))?;
        let target_type = transform_rule
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("string");

        // `source_path == "."` means "the whole source document" — lets a
        // nested CIF object collect root-level fields of a flat source.
        let source_value = if source_path == "." {
            Some(source)
        } else {
            Self::extract_value_by_path(source, source_path)
        };
        let Some(source_value) = source_value else {
            let is_required = cif_field_def
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if is_required {
                return Err(format!(
                    "Required field '{cif_field_name}' not found at path '{source_path}'"
                )
                .into());
            }
            return Ok(());
        };

        // Array with a declared element shape: walk the source array and
        // emit one CIF element per source element. This is what lets cross-
        // system array fields (e.g. `items`) carry both sides' stable
        // anchors in one canonical element type.
        if target_type == "array"
            && let Some(element_rules) = transform_rule.get("element")
        {
            let element_schema = cif_field_def.get("element");
            let out_arr = Self::transform_array(
                source_value,
                element_rules,
                element_schema,
                cif_field_name,
                source_path,
            )?;
            cif_object.insert(cif_field_name.to_string(), Value::Array(out_arr));
            return Ok(());
        }

        // Object with a declared children shape: same rule shape as
        // `element`, applied once to the resolved value rather than per
        // array element. Lets a cross-system nested object (e.g.
        // `supplier`) compose from several source paths into one CIF object.
        if target_type == "object"
            && let Some(child_rules) = transform_rule.get("children")
        {
            let child_schema = cif_field_def.get("children");
            let out_obj = Self::transform_element(
                source_value,
                child_rules,
                child_schema,
                cif_field_name,
                "children",
            )?;
            cif_object.insert(cif_field_name.to_string(), out_obj);
            return Ok(());
        }

        // Scalar / opaque-array fallback (no element shape).
        let normalized_value = Self::normalize_type(source_value, target_type)?;
        cif_object.insert(cif_field_name.to_string(), normalized_value);
        Ok(())
    }

    /// Walk a source array and emit one CIF element per source element.
    fn transform_array(
        source_value: &Value,
        element_rules: &Value,
        element_schema: Option<&Value>,
        parent_field: &str,
        source_path: &str,
    ) -> Result<Vec<Value>, Box<dyn Error>> {
        let arr = source_value.as_array().ok_or_else(|| {
            format!(
                "source value at '{source_path}' for field '{parent_field}' is not an array"
            )
        })?;
        let mut out = Vec::with_capacity(arr.len());
        for elem in arr {
            out.push(Self::transform_element(
                elem,
                element_rules,
                element_schema,
                parent_field,
                "element",
            )?);
        }
        Ok(out)
    }

    /// Transform a single source value against a set of field rules. Serves
    /// both array `element` rules (called once per array element by
    /// [`Self::transform_array`]) and object `children` rules (called once
    /// against the resolved child value) — the rule shape and relative
    /// `source_path` semantics are identical; `kind` ("element" | "children")
    /// only labels error text. Each rule value may itself be
    /// `{source_path, type, element?/children?}` (recursive), so arrays and
    /// objects compose at any depth.
    fn transform_element(
        source_elem: &Value,
        element_rules: &Value,
        element_schema: Option<&Value>,
        parent_field: &str,
        kind: &str,
    ) -> Result<Value, Box<dyn Error>> {
        let rules_obj = element_rules.as_object().ok_or_else(|| {
            format!("transformation.{kind} for '{parent_field}' must be an object")
        })?;

        let mut out = serde_json::Map::new();
        for (elem_field, rule) in rules_obj {
            let sub_source_path = rule
                .get("source_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("{kind}.{elem_field} for '{parent_field}' is missing source_path")
                })?;
            let sub_target_type = rule.get("type").and_then(Value::as_str).unwrap_or("string");

            // `source_path == "."` means "take the element itself" — useful
            // when the source array holds scalars rather than objects.
            let sub_source_value = if sub_source_path == "." {
                Some(source_elem)
            } else {
                Self::extract_value_by_path(source_elem, sub_source_path)
            };

            let Some(val) = sub_source_value else {
                let is_required = element_schema
                    .and_then(|s| s.get(elem_field))
                    .and_then(|f| f.get("required"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if is_required {
                    return Err(format!(
                        "required {kind} field '{parent_field}.{elem_field}' missing at \
                         source_path '{sub_source_path}'"
                    )
                    .into());
                }
                continue;
            };

            let normalized = if sub_target_type == "array"
                && let Some(nested_rules) = rule.get("element")
            {
                let nested_schema = element_schema
                    .and_then(|s| s.get(elem_field))
                    .and_then(|f| f.get("element"));
                let nested_parent = format!("{parent_field}.{elem_field}");
                let nested_out = Self::transform_array(
                    val,
                    nested_rules,
                    nested_schema,
                    &nested_parent,
                    sub_source_path,
                )?;
                Value::Array(nested_out)
            } else if sub_target_type == "object"
                && let Some(nested_rules) = rule.get("children")
            {
                let nested_schema = element_schema
                    .and_then(|s| s.get(elem_field))
                    .and_then(|f| f.get("children"));
                let nested_parent = format!("{parent_field}.{elem_field}");
                Self::transform_element(val, nested_rules, nested_schema, &nested_parent, "children")?
            } else {
                Self::normalize_type(val, sub_target_type)?
            };
            out.insert(elem_field.clone(), normalized);
        }
        Ok(Value::Object(out))
    }

    /// Resolve a dotted path against `json`. Numeric segments index arrays
    /// (e.g. `items.0.name`); non-numeric segments index objects.
    fn extract_value_by_path<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
        let mut current = json;
        for part in split_path(path) {
            current = match current {
                Value::Object(map) => map.get(&part)?,
                Value::Array(arr) => arr.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(current)
    }

    fn normalize_type(value: &Value, target_type: &str) -> Result<Value, Box<dyn Error>> {
        match target_type {
            "string" => Ok(match value {
                Value::String(s) => Value::String(s.clone()),
                Value::Number(n) => Value::String(n.to_string()),
                Value::Bool(b) => Value::String(b.to_string()),
                _ => Value::String(value.to_string()),
            }),
            "number" => Ok(match value {
                Value::Number(n) => Value::Number(n.clone()),
                Value::String(s) => {
                    let num: f64 = s.parse()?;
                    serde_json::json!(num)
                }
                _ => return Err(format!("Cannot convert {value:?} to number").into()),
            }),
            "boolean" => Ok(match value {
                Value::Bool(b) => Value::Bool(*b),
                Value::String(s) => Value::Bool(s.eq_ignore_ascii_case("true")),
                Value::Number(n) => Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0),
                _ => return Err(format!("Cannot convert {value:?} to boolean").into()),
            }),
            _ => Ok(value.clone()),
        }
    }
}

/// Transform a JSON document to CIF using the schema's `transformations`
/// rules for `format_id`.
///
/// # Examples
/// ```
/// use diff_fusion::transform_to_cif;
/// use serde_json::json;
///
/// let source = json!({"name": "Widget", "price": 19.99});
/// let schema = json!({
///     "cif_schema": {
///         "product_name": {"type": "string", "required": true}
///     },
///     "transformations": {
///         "format_a": {
///             "product_name": {"source_path": "name", "type": "string"}
///         }
///     }
/// });
///
/// let result = transform_to_cif(&source, &schema, "format_a");
/// ```
pub fn transform_to_cif(
    source: &Value,
    schema: &Value,
    format_id: &str,
) -> Result<Value, Box<dyn Error>> {
    Transformer::to_cif(source, schema, format_id)
}
