// Example: Hybrid approach - Grouping without state
// This combines the discoverability of OOP with the benefits of functional programming

use serde_json::{Value, json};

// ==========================================
// Option 1: Zero-Cost Stateless Struct
// ==========================================
// Acts like a namespace, but with method syntax

pub struct Transformer;

impl Transformer {
    /// Transform to CIF format
    pub fn to_cif(source: &Value, schema: &Value, format_id: &str) -> Result<Value, String> {
        // Pure function - no access to self, no state
        // Just a namespace for grouping related functions
        Self::validate_schema(schema)?;
        Self::apply_transformations(source, schema, format_id)
    }

    fn validate_schema(schema: &Value) -> Result<(), String> {
        schema.get("cif_schema").ok_or("Missing cif_schema")?;
        Ok(())
    }

    fn apply_transformations(
        source: &Value,
        schema: &Value,
        format_id: &str,
    ) -> Result<Value, String> {
        // Implementation here
        Ok(json!({}))
    }
}

pub struct Comparator;

impl Comparator {
    /// Compare two JSON values
    pub fn compare(old: &Value, new: &Value) -> Vec<String> {
        // Pure function, grouped under Comparator namespace
        Self::find_differences(old, new, "")
    }

    fn find_differences(old: &Value, new: &Value, path: &str) -> Vec<String> {
        // Helper function
        vec![]
    }
}

// ==========================================
// Option 2: Builder Pattern with Immutable Config
// ==========================================
// If you need to carry configuration

pub struct TransformerConfig {
    strict_mode: bool,
    allow_missing: bool,
}

impl TransformerConfig {
    pub fn new() -> Self {
        Self {
            strict_mode: true,
            allow_missing: false,
        }
    }

    pub fn strict(mut self, enabled: bool) -> Self {
        self.strict_mode = enabled;
        self
    }

    /// Transform with this configuration
    pub fn transform(
        &self, // Immutable borrow - functional!
        source: &Value,
        schema: &Value,
        format_id: &str,
    ) -> Result<Value, String> {
        // Uses self.config but doesn't mutate it
        if self.strict_mode {
            // Strict validation
        }
        Ok(json!({}))
    }
}

// ==========================================
// Option 3: Trait-Based Grouping
// ==========================================
// Group operations by trait

pub trait JsonTransform {
    fn to_cif(&self, schema: &Value, format_id: &str) -> Result<Value, String>;
    fn normalize(&self) -> Value;
}

impl JsonTransform for Value {
    fn to_cif(&self, schema: &Value, format_id: &str) -> Result<Value, String> {
        // Now you can do: my_json.to_cif(schema, "format_a")
        Ok(json!({}))
    }

    fn normalize(&self) -> Value {
        // Helper methods on the type itself
        self.clone()
    }
}

pub trait JsonCompare {
    fn diff(&self, other: &Value) -> Vec<String>;
}

impl JsonCompare for Value {
    fn diff(&self, other: &Value) -> Vec<String> {
        // my_json.diff(&other_json)
        vec![]
    }
}

fn main() {
    let source = json!({"name": "Widget"});
    let schema = json!({"cif_schema": {}});

    // ==========================================
    // Usage Examples - All are ergonomic!
    // ==========================================

    // 1. Stateless struct (namespace-like)
    let result = Transformer::to_cif(&source, &schema, "format_a");
    let diffs = Comparator::compare(&source, &source);

    // 2. Builder pattern (config + functional)
    let result = TransformerConfig::new()
        .strict(false)
        .transform(&source, &schema, "format_a");

    // 3. Trait extension methods
    let result = source.to_cif(&schema, "format_a");
    let diffs = source.diff(&source);

    println!("✅ All approaches provide good grouping AND functional benefits!");
}
