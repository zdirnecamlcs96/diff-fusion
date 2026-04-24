use serde_json::Value;
use std::error::Error;

/// Transformer provides methods for converting JSON to Common Intermediate Format (CIF)
///
/// This is a zero-cost abstraction - no runtime overhead, just better organization.
/// All methods are pure functions (no state, no side effects).
pub struct Transformer;

impl Transformer {
    /// Transform a JSON document to Common Intermediate Format (CIF)
    /// based on the schema definition and format identifier
    ///
    /// # Examples
    /// ```
    /// use diff_fusion::Transformer;
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
    /// let result = Transformer::to_cif(&source, &schema, "format_a");
    /// ```
    pub fn to_cif(
        source: &Value,
        schema: &Value,
        format_id: &str,
    ) -> Result<Value, Box<dyn Error>> {
        let transformations = Self::get_transformations(schema, format_id)?;
        let cif_schema = Self::get_cif_schema(schema)?;

        let mut cif_object = serde_json::Map::new();

        // For each field in the CIF schema, extract and transform from source
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

    /// Get transformations for a specific format from schema
    fn get_transformations<'a>(
        schema: &'a Value,
        format_id: &str,
    ) -> Result<&'a Value, Box<dyn Error>> {
        schema
            .get("transformations")
            .and_then(|t| t.get(format_id))
            .ok_or_else(|| format!("Format '{}' not found in schema", format_id).into())
    }

    /// Get CIF schema definition
    fn get_cif_schema(schema: &Value) -> Result<&Value, Box<dyn Error>> {
        schema
            .get("cif_schema")
            .ok_or("'cif_schema' not defined in schema".into())
    }

    /// Transform a single field from source to CIF
    fn transform_field(
        source: &Value,
        transformations: &Value,
        cif_field_name: &str,
        cif_field_def: &Value,
        cif_object: &mut serde_json::Map<String, Value>,
    ) -> Result<(), Box<dyn Error>> {
        // Get transformation rule for this CIF field
        if let Some(transform_rule) = transformations.get(cif_field_name) {
            let source_path = transform_rule
                .get("source_path")
                .and_then(Value::as_str)
                .ok_or(format!(
                    "source_path not defined for field '{}'",
                    cif_field_name
                ))?;

            let target_type = transform_rule
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("string");

            // Extract value from source JSON
            if let Some(source_value) = Self::extract_value_by_path(source, source_path) {
                // Array of objects with a declared element shape: walk the
                // source array and emit one CIF element per source element,
                // using the element-level transformation rules. This is what
                // lets cross-system array fields (e.g. `items`) carry both
                // sides' stable anchors in a single canonical element type.
                if target_type == "array" {
                    if let Some(element_rules) = transform_rule.get("element") {
                        let element_schema = cif_field_def.get("element");
                        let arr = source_value.as_array().ok_or_else(|| {
                            format!(
                                "source value at '{source_path}' for field '{cif_field_name}' \
                                 is not an array"
                            )
                        })?;
                        let mut out_arr = Vec::with_capacity(arr.len());
                        for elem in arr {
                            out_arr.push(Self::transform_element(
                                elem,
                                element_rules,
                                element_schema,
                                cif_field_name,
                            )?);
                        }
                        cif_object.insert(cif_field_name.to_string(), Value::Array(out_arr));
                        return Ok(());
                    }
                }
                // Scalar / opaque-array fallback (no element shape).
                let normalized_value = Self::normalize_type(source_value, target_type)?;
                cif_object.insert(cif_field_name.to_string(), normalized_value);
            } else {
                // Handle required fields
                let is_required = cif_field_def
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);

                if is_required {
                    return Err(format!(
                        "Required field '{}' not found at path '{}'",
                        cif_field_name, source_path
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Transform a single source array element according to element rules.
    /// Each rule key names a CIF element field; each rule value may itself
    /// be `{source_path, type, element?}` (recursive) so arrays of arrays
    /// of objects are expressible.
    fn transform_element(
        source_elem: &Value,
        element_rules: &Value,
        element_schema: Option<&Value>,
        parent_field: &str,
    ) -> Result<Value, Box<dyn Error>> {
        let rules_obj = element_rules.as_object().ok_or_else(|| {
            format!("transformation.element for '{parent_field}' must be an object")
        })?;

        let mut out = serde_json::Map::new();
        for (elem_field, rule) in rules_obj {
            let sub_source_path = rule
                .get("source_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "element.{elem_field} for '{parent_field}' is missing source_path"
                    )
                })?;
            let sub_target_type = rule.get("type").and_then(Value::as_str).unwrap_or("string");

            // `source_path == "."` means "take the element itself" — useful
            // when the source array holds scalars rather than objects.
            let sub_source_value: Option<&Value> = if sub_source_path == "." {
                Some(source_elem)
            } else {
                Self::extract_value_by_path(source_elem, sub_source_path)
            };

            if let Some(val) = sub_source_value {
                let normalized = if sub_target_type == "array" && rule.get("element").is_some() {
                    let nested_rules = rule.get("element").unwrap();
                    let nested_schema = element_schema
                        .and_then(|s| s.get(elem_field))
                        .and_then(|f| f.get("element"));
                    let arr = val.as_array().ok_or_else(|| {
                        format!(
                            "nested array '{parent_field}.{elem_field}' source is not an array"
                        )
                    })?;
                    let mut nested_out = Vec::with_capacity(arr.len());
                    for sub in arr {
                        nested_out.push(Self::transform_element(
                            sub,
                            nested_rules,
                            nested_schema,
                            &format!("{parent_field}.{elem_field}"),
                        )?);
                    }
                    Value::Array(nested_out)
                } else {
                    Self::normalize_type(val, sub_target_type)?
                };
                out.insert(elem_field.clone(), normalized);
            } else {
                // Element-level required check mirrors top-level behaviour.
                let is_required = element_schema
                    .and_then(|s| s.get(elem_field))
                    .and_then(|f| f.get("required"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if is_required {
                    return Err(format!(
                        "required element field '{parent_field}.{elem_field}' missing at \
                         source_path '{sub_source_path}'"
                    )
                    .into());
                }
            }
        }
        Ok(Value::Object(out))
    }

    /// Extract a value from JSON by dot-notation path (e.g., "user.name")
    fn extract_value_by_path<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for part in parts {
            match current {
                Value::Object(map) => {
                    current = map.get(part)?;
                }
                Value::Array(arr) => {
                    // Support array indexing like "items.0.name"
                    if let Ok(index) = part.parse::<usize>() {
                        current = arr.get(index)?;
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Normalize value to target type
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
                _ => return Err(format!("Cannot convert {:?} to number", value).into()),
            }),
            "boolean" => Ok(match value {
                Value::Bool(b) => Value::Bool(*b),
                Value::String(s) => Value::Bool(s.to_lowercase() == "true"),
                Value::Number(n) => Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0),
                _ => return Err(format!("Cannot convert {:?} to boolean", value).into()),
            }),
            _ => Ok(value.clone()),
        }
    }
}

// Keep the old function names for backward compatibility
pub fn transform_to_cif(
    source: &Value,
    schema: &Value,
    format_id: &str,
) -> Result<Value, Box<dyn Error>> {
    Transformer::to_cif(source, schema, format_id)
}
