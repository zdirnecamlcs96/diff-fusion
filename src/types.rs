use serde::{Deserialize, Serialize};
use std::fmt;

/// Conflict resolution strategies for fields without explicit source of truth
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Use the most recent value (requires timestamp metadata)
    LastWriteWins,
    /// Always prefer System A's value
    PreferA,
    /// Always prefer System B's value
    PreferB,
    /// Manual resolution required (raise error)
    ManualResolve,
    /// Use the larger/newer value (for numbers/dates)
    UseMax,
    /// Use the smaller/older value (for numbers/dates)
    UseMin,
    /// Merge values if possible (for arrays/objects)
    Merge,
}

/// Base types supported in CIF schema
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CifType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Null,
}

impl fmt::Display for CifType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CifType::String => write!(f, "string"),
            CifType::Number => write!(f, "number"),
            CifType::Boolean => write!(f, "boolean"),
            CifType::Object => write!(f, "object"),
            CifType::Array => write!(f, "array"),
            CifType::Null => write!(f, "null"),
        }
    }
}

impl CifType {
    /// Parse type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "string" => Some(CifType::String),
            "number" => Some(CifType::Number),
            "boolean" => Some(CifType::Boolean),
            "object" => Some(CifType::Object),
            "array" => Some(CifType::Array),
            "null" => Some(CifType::Null),
            _ => None,
        }
    }

    /// Check if type allows null values
    pub fn is_nullable(&self) -> bool {
        matches!(self, CifType::Null)
    }

    /// Check if type is a primitive (string, number, boolean)
    pub fn is_primitive(&self) -> bool {
        matches!(self, CifType::String | CifType::Number | CifType::Boolean)
    }

    /// Check if type is a collection (array or object)
    pub fn is_collection(&self) -> bool {
        matches!(self, CifType::Array | CifType::Object)
    }
}

/// Field definition in CIF schema
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CifFieldDefinition {
    /// The type of this field
    #[serde(rename = "type")]
    pub field_type: String,

    /// Whether this field is required
    pub required: bool,

    /// Optional description for documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Source of truth for this field (e.g., "inventory_system", "pricing_system")
    /// When conflicts occur, this system's value is authoritative
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_of_truth: Option<String>,

    /// Conflict resolution strategy when source_of_truth is not specified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_strategy: Option<ConflictStrategy>,
}

impl CifFieldDefinition {
    /// Create a new field definition (defaults to optional)
    ///
    /// # Examples
    /// ```
    /// use diff_fusion::types::CifFieldDefinition;
    /// use serde_json::json;
    ///
    /// // Simple required field
    /// let field = CifFieldDefinition::new("string").required();
    ///
    /// // Optional field with default
    /// let field = CifFieldDefinition::new("number")
    ///     .with_default(json!(0));
    ///
    /// // Required field with description
    /// let field = CifFieldDefinition::new("string")
    ///     .required()
    ///     .with_description("User's email address");
    /// ```
    pub fn new(field_type: &str) -> Self {
        Self {
            field_type: field_type.to_string(),
            required: false,
            description: None,
            default: None,
            source_of_truth: None,
            conflict_strategy: None,
        }
    }

    /// Make this field required (chainable)
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Make this field optional (chainable)
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Add a description to this field (chainable)
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Add a default value to this field
    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }

    /// Set the source of truth for this field (chainable)
    ///
    /// When conflicts occur during sync, the value from this system is authoritative.
    ///
    /// # Examples
    /// ```
    /// use diff_fusion::types::CifFieldDefinition;
    ///
    /// let field = CifFieldDefinition::new("number")
    ///     .required()
    ///     .with_source_of_truth("inventory_system")
    ///     .with_description("Stock quantity - inventory is source of truth");
    /// ```
    pub fn with_source_of_truth(mut self, system: &str) -> Self {
        self.source_of_truth = Some(system.to_string());
        self
    }

    /// Set the conflict resolution strategy (chainable)
    ///
    /// Used when source_of_truth is not specified. Determines how to handle
    /// conflicts between systems.
    ///
    /// # Examples
    /// ```
    /// use diff_fusion::types::{CifFieldDefinition, ConflictStrategy};
    ///
    /// let field = CifFieldDefinition::new("number")
    ///     .with_conflict_strategy(ConflictStrategy::LastWriteWins)
    ///     .with_description("Price - use most recent value");
    /// ```
    pub fn with_conflict_strategy(mut self, strategy: ConflictStrategy) -> Self {
        self.conflict_strategy = Some(strategy);
        self
    }

    /// Get the parsed CifType
    pub fn get_type(&self) -> Option<CifType> {
        CifType::from_str(&self.field_type)
    }

    /// Validate that the field definition is valid
    pub fn validate(&self) -> Result<(), String> {
        // Check if type is supported
        if CifType::from_str(&self.field_type).is_none() {
            return Err(format!("Unsupported type: {}", self.field_type));
        }

        // Check if required fields don't have defaults (they shouldn't need them)
        if self.required && self.default.is_some() {
            return Err("Required fields should not have default values".to_string());
        }

        Ok(())
    }
}

/// Trait for types that can be used as CIF schemas
///
/// This allows compile-time type safety for CIF schemas, instead of relying
/// on runtime JSON validation. Implement this trait for your domain types
/// to get automatic schema generation and validation.
///
/// # Examples
/// ```
/// use diff_fusion::types::{CifSchema, CifFieldDefinition};
/// use serde_json::json;
///
/// struct User {
///     email: String,
///     age: u32,
/// }
///
/// impl CifSchema for User {
///     fn schema_name() -> &'static str {
///         "user"
///     }
///
///     fn fields() -> Vec<(&'static str, CifFieldDefinition)> {
///         vec![
///             ("email", CifFieldDefinition::new("string")
///                 .required()
///                 .with_description("User's email address")),
///             ("age", CifFieldDefinition::new("number")
///                 .required()
///                 .with_description("User's age")),
///         ]
///     }
/// }
///
/// // Now you can generate JSON schema from the type
/// let schema_json = User::to_json_schema();
/// ```
pub trait CifSchema {
    /// Get the schema name
    fn schema_name() -> &'static str;

    /// Get the field definitions
    fn fields() -> Vec<(&'static str, CifFieldDefinition)>;

    /// Convert to JSON schema format (compatible with existing JSON schemas)
    fn to_json_schema() -> serde_json::Value {
        let mut schema_obj = serde_json::Map::new();

        for (field_name, field_def) in Self::fields() {
            schema_obj.insert(
                field_name.to_string(),
                serde_json::json!({
                    "type": field_def.field_type,
                    "required": field_def.required,
                    "description": field_def.description,
                    "default": field_def.default,
                }),
            );
        }

        serde_json::json!({
            "cif_schema": schema_obj
        })
    }

    /// Validate a JSON value against this schema
    fn validate(value: &serde_json::Value) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if !value.is_object() {
            errors.push("Value must be an object".to_string());
            return Err(errors);
        }

        let obj = value.as_object().unwrap();

        for (field_name, field_def) in Self::fields() {
            // Check required fields
            if field_def.required && !obj.contains_key(field_name) {
                errors.push(format!("Missing required field: {}", field_name));
            }

            // Validate types if field exists
            if let Some(field_value) = obj.get(field_name) {
                if let Err(err) = validate_field_type(field_value, &field_def.field_type) {
                    errors.push(format!("Field '{}': {}", field_name, err));
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

/// Helper function to validate field types
fn validate_field_type(value: &serde_json::Value, expected_type: &str) -> Result<(), String> {
    let actual_type = match value {
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Null => "null",
    };

    if actual_type == expected_type {
        Ok(())
    } else {
        Err(format!(
            "Expected type '{}', got '{}'",
            expected_type, actual_type
        ))
    }
}

/// Transformation mapping for a field
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldTransformation {
    /// Path to extract value from source (dot notation supported)
    pub source_path: String,

    /// Target type in CIF
    #[serde(rename = "type")]
    pub target_type: String,

    /// Optional transformation notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl FieldTransformation {
    /// Create a new field transformation
    pub fn new(source_path: &str, target_type: &str) -> Self {
        Self {
            source_path: source_path.to_string(),
            target_type: target_type.to_string(),
            notes: None,
        }
    }

    /// Add notes to this transformation
    pub fn with_notes(mut self, notes: &str) -> Self {
        self.notes = Some(notes.to_string());
        self
    }

    /// Get the parsed CifType
    pub fn get_type(&self) -> Option<CifType> {
        CifType::from_str(&self.target_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cif_type_from_str() {
        assert_eq!(CifType::from_str("string"), Some(CifType::String));
        assert_eq!(CifType::from_str("number"), Some(CifType::Number));
        assert_eq!(CifType::from_str("boolean"), Some(CifType::Boolean));
        assert_eq!(CifType::from_str("invalid"), None);
    }

    #[test]
    fn test_cif_type_display() {
        assert_eq!(CifType::String.to_string(), "string");
        assert_eq!(CifType::Number.to_string(), "number");
        assert_eq!(CifType::Boolean.to_string(), "boolean");
    }

    #[test]
    fn test_cif_type_predicates() {
        assert!(CifType::String.is_primitive());
        assert!(!CifType::Array.is_primitive());
        assert!(CifType::Array.is_collection());
        assert!(!CifType::String.is_collection());
    }

    #[test]
    fn test_field_definition_required() {
        let field = CifFieldDefinition::new("string").required();
        assert_eq!(field.field_type, "string");
        assert!(field.required);
        assert!(field.description.is_none());
    }

    #[test]
    fn test_field_definition_optional() {
        let field = CifFieldDefinition::new("number").optional();
        assert_eq!(field.field_type, "number");
        assert!(!field.required);
    }

    #[test]
    fn test_field_definition_with_description() {
        let field = CifFieldDefinition::new("string")
            .required()
            .with_description("Product name");
        assert_eq!(field.description, Some("Product name".to_string()));
    }

    #[test]
    fn test_field_definition_builder_pattern() {
        // Test chaining multiple methods
        let field = CifFieldDefinition::new("string")
            .required()
            .with_description("Email address")
            .with_default(serde_json::json!("user@example.com"));

        assert_eq!(field.field_type, "string");
        assert!(field.required);
        assert_eq!(field.description, Some("Email address".to_string()));
        assert_eq!(field.default, Some(serde_json::json!("user@example.com")));
    }

    #[test]
    fn test_field_definition_validate() {
        let valid = CifFieldDefinition::new("string").required();
        assert!(valid.validate().is_ok());

        let invalid = CifFieldDefinition {
            field_type: "invalid_type".to_string(),
            required: true,
            description: None,
            default: None,
            source_of_truth: None,
            conflict_strategy: None,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_source_of_truth() {
        let field = CifFieldDefinition::new("number")
            .required()
            .with_source_of_truth("inventory_system")
            .with_description("Stock level");

        assert_eq!(field.source_of_truth, Some("inventory_system".to_string()));
        assert_eq!(field.description, Some("Stock level".to_string()));
    }

    #[test]
    fn test_conflict_strategy() {
        let field = CifFieldDefinition::new("number")
            .with_conflict_strategy(ConflictStrategy::LastWriteWins)
            .with_description("Price");

        assert_eq!(
            field.conflict_strategy,
            Some(ConflictStrategy::LastWriteWins)
        );
    }

    #[test]
    fn test_field_transformation() {
        let transform = FieldTransformation::new("name", "string").with_notes("Maps product name");

        assert_eq!(transform.source_path, "name");
        assert_eq!(transform.target_type, "string");
        assert_eq!(transform.notes, Some("Maps product name".to_string()));
    }

    #[test]
    fn test_cif_schema_trait() {
        use serde_json::json;

        // Define a test schema
        struct TestUser;
        impl CifSchema for TestUser {
            fn schema_name() -> &'static str {
                "test_user"
            }

            fn fields() -> Vec<(&'static str, CifFieldDefinition)> {
                vec![
                    (
                        "email",
                        CifFieldDefinition::new("string")
                            .required()
                            .with_description("User email"),
                    ),
                    (
                        "age",
                        CifFieldDefinition::new("number")
                            .optional()
                            .with_default(json!(0)),
                    ),
                ]
            }
        }

        // Test schema name
        assert_eq!(TestUser::schema_name(), "test_user");

        // Test JSON schema generation
        let schema = TestUser::to_json_schema();
        assert!(schema.get("cif_schema").is_some());
        assert!(schema["cif_schema"].get("email").is_some());
        assert!(schema["cif_schema"].get("age").is_some());

        // Test valid data validation
        let valid_data = json!({
            "email": "test@example.com",
            "age": 25
        });
        assert!(TestUser::validate(&valid_data).is_ok());

        // Test missing required field
        let invalid_data = json!({
            "age": 25
        });
        let result = TestUser::validate(&invalid_data);
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("Missing required field"));

        // Test type mismatch
        let wrong_type = json!({
            "email": "test@example.com",
            "age": "not a number"
        });
        let result = TestUser::validate(&wrong_type);
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("Expected type 'number'"));
    }

    #[test]
    fn test_validate_field_type() {
        use serde_json::json;

        assert!(validate_field_type(&json!("hello"), "string").is_ok());
        assert!(validate_field_type(&json!(42), "number").is_ok());
        assert!(validate_field_type(&json!(true), "boolean").is_ok());
        assert!(validate_field_type(&json!({}), "object").is_ok());
        assert!(validate_field_type(&json!([]), "array").is_ok());
        assert!(validate_field_type(&json!(null), "null").is_ok());

        // Type mismatches
        assert!(validate_field_type(&json!("hello"), "number").is_err());
        assert!(validate_field_type(&json!(42), "string").is_err());
    }
}
