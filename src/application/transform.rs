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
                // Type conversion/normalization
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
