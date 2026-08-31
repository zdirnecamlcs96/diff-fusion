use serde_json::Value;
use std::error::Error;

use crate::domain::compare::compare_json;
use crate::application::transform::Transformer;
use crate::{Conflict, ConflictReport};

/// Main entry point for the diff-fusion library
///
/// This provides a high-level API that hides all internal complexity.
/// Users interact only with this facade - no need to understand the
/// underlying pure functions or implementation details.
///
/// # Examples
///
/// ```
/// use diff_fusion::DiffFusion;
/// use serde_json::json;
///
/// // 1. Create a facade with your schema
/// let schema = json!({
///     "cif_schema": {
///         "product_name": {"type": "string", "required": true},
///         "price": {"type": "number", "required": true}
///     },
///     "transformations": {
///         "format_a": {
///             "product_name": {"source_path": "name", "type": "string"},
///             "price": {"source_path": "price", "type": "number"}
///         }
///     }
/// });
///
/// let diff_fusion = DiffFusion::new(schema);
///
/// // 2. Transform data to CIF
/// let source = json!({"name": "Widget", "price": 19.99});
/// let cif = diff_fusion.transform(&source, "format_a").unwrap();
///
/// // 3. Compare two CIF objects for conflicts
/// let old_data = json!({"product_name": "Widget", "price": 19.99});
/// let new_data = json!({"product_name": "Widget", "price": 24.99});
/// let report = diff_fusion.compare(&old_data, &new_data);
/// ```
pub struct DiffFusion {
    schema: Value,
}

impl DiffFusion {
    /// Create a new DiffFusion instance with the given schema
    ///
    /// The schema defines:
    /// - `cif_schema`: The Common Intermediate Format field definitions
    /// - `transformations`: Mapping rules for each format
    pub fn new(schema: Value) -> Self {
        Self { schema }
    }

    /// Transform source data to Common Intermediate Format (CIF)
    ///
    /// # Arguments
    /// - `source`: The source data to transform
    /// - `format_id`: The format identifier (e.g., "format_a", "system_salesforce")
    ///
    /// # Returns
    /// Transformed data in CIF format, or an error if transformation fails
    pub fn transform(&self, source: &Value, format_id: &str) -> Result<Value, Box<dyn Error>> {
        Transformer::to_cif(source, &self.schema, format_id)
    }

    /// Compare two JSON values and detect conflicts
    ///
    /// Returns a detailed conflict report showing what changed between
    /// the old and new values.
    pub fn compare(&self, old: &Value, new: &Value) -> ConflictReport {
        let diffs = compare_json(old, new);

        let conflicts: Vec<Conflict> = diffs
            .into_iter()
            .map(|(path, (old_val, new_val))| Conflict {
                path,
                old_value: format!("{:?}", old_val),
                new_value: format!("{:?}", new_val),
            })
            .collect();

        ConflictReport {
            has_conflicts: !conflicts.is_empty(),
            total_conflicts: conflicts.len(),
            conflicts,
        }
    }

    /// End-to-end workflow: Transform two sources and compare them
    ///
    /// This is useful when you want to:
    /// 1. Transform data from format A to CIF
    /// 2. Transform data from format B to CIF
    /// 3. Compare them for conflicts
    ///
    /// # Examples
    /// ```
    /// use diff_fusion::DiffFusion;
    /// use serde_json::json;
    ///
    /// # let schema = json!({
    /// #     "cif_schema": {"name": {"type": "string", "required": true}},
    /// #     "transformations": {
    /// #         "format_a": {"name": {"source_path": "name", "type": "string"}},
    /// #         "format_b": {"name": {"source_path": "full_name", "type": "string"}}
    /// #     }
    /// # });
    /// let diff_fusion = DiffFusion::new(schema);
    ///
    /// let source_a = json!({"name": "Alice"});
    /// let source_b = json!({"full_name": "Bob"});
    ///
    /// let report = diff_fusion.transform_and_compare(
    ///     &source_a, "format_a",
    ///     &source_b, "format_b"
    /// ).unwrap();
    ///
    /// println!("Conflicts: {}", report.total_conflicts);
    /// ```
    pub fn transform_and_compare(
        &self,
        source_a: &Value,
        format_a: &str,
        source_b: &Value,
        format_b: &str,
    ) -> Result<ConflictReport, Box<dyn Error>> {
        let cif_a = self.transform(source_a, format_a)?;
        let cif_b = self.transform(source_b, format_b)?;
        Ok(self.compare(&cif_a, &cif_b))
    }

    /// Get the schema being used
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// Validate that a value matches the CIF schema
    ///
    /// Checks if all required fields are present and have correct types
    pub fn validate_cif(&self, value: &Value) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        let Some(cif_schema) = self.schema.get("cif_schema") else {
            return Err(vec!["Schema missing 'cif_schema' definition".to_string()]);
        };

        let Some(value_obj) = value.as_object() else {
            return Err(vec!["CIF value must be an object".to_string()]);
        };

        let cif_fields = cif_schema
            .as_object()
            .ok_or_else(|| vec!["cif_schema must be an object".to_string()])?;

        for (field_name, field_def) in cif_fields {
            // Check required fields
            let is_required = field_def
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if is_required && !value_obj.contains_key(field_name) {
                errors.push(format!("Missing required field: {}", field_name));
            }

            // Type validation (basic)
            if let Some(field_value) = value_obj.get(field_name)
                && let Some(expected_type) = field_def.get("type").and_then(|v| v.as_str())
            {
                let actual_type = match field_value {
                    Value::String(_) => "string",
                    Value::Number(_) => "number",
                    Value::Bool(_) => "boolean",
                    Value::Object(_) => "object",
                    Value::Array(_) => "array",
                    Value::Null => "null",
                };

                if actual_type != expected_type {
                    errors.push(format!(
                        "Field '{}': expected type '{}', got '{}'",
                        field_name, expected_type, actual_type
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn get_test_schema() -> Value {
        json!({
            "cif_schema": {
                "product_name": {"type": "string", "required": true},
                "price": {"type": "number", "required": true}
            },
            "transformations": {
                "format_a": {
                    "product_name": {"source_path": "name", "type": "string"},
                    "price": {"source_path": "price", "type": "number"}
                }
            }
        })
    }

    #[test]
    fn test_facade_creation() {
        let schema = get_test_schema();
        let diff_fusion = DiffFusion::new(schema);
        assert!(diff_fusion.schema().get("cif_schema").is_some());
    }

    #[test]
    fn test_facade_transform() {
        let diff_fusion = DiffFusion::new(get_test_schema());
        let source = json!({"name": "Widget", "price": 19.99});

        let result = diff_fusion.transform(&source, "format_a");
        assert!(result.is_ok());

        let cif = result.unwrap();
        assert_eq!(cif["product_name"], "Widget");
        assert_eq!(cif["price"], 19.99);
    }

    #[test]
    fn test_facade_compare() {
        let diff_fusion = DiffFusion::new(get_test_schema());
        let old = json!({"product_name": "Widget", "price": 19.99});
        let new = json!({"product_name": "Widget", "price": 24.99});

        let report = diff_fusion.compare(&old, &new);
        assert!(report.has_conflicts);
        assert_eq!(report.total_conflicts, 1);
        assert_eq!(report.conflicts[0].path, "price");
    }

    #[test]
    fn test_facade_transform_and_compare() {
        let diff_fusion = DiffFusion::new(get_test_schema());
        let source_a = json!({"name": "Widget", "price": 19.99});
        let source_b = json!({"name": "Widget", "price": 24.99});

        let result =
            diff_fusion.transform_and_compare(&source_a, "format_a", &source_b, "format_a");

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.has_conflicts);
    }

    #[test]
    fn test_validate_cif() {
        let diff_fusion = DiffFusion::new(get_test_schema());

        // Valid CIF
        let valid = json!({"product_name": "Widget", "price": 19.99});
        assert!(diff_fusion.validate_cif(&valid).is_ok());

        // Missing required field
        let invalid = json!({"product_name": "Widget"});
        assert!(diff_fusion.validate_cif(&invalid).is_err());

        // Wrong type
        let wrong_type = json!({"product_name": "Widget", "price": "not a number"});
        assert!(diff_fusion.validate_cif(&wrong_type).is_err());
    }
}
