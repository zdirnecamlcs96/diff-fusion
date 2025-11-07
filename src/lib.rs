pub mod compare;
pub mod transform;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// Re-export core functions for library users
pub use compare::compare_json;
pub use transform::transform_to_cif;

/// Conflict detected between two JSON values
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Conflict {
    pub path: String,
    pub old_value: String,
    pub new_value: String,
}

/// Summary of conflict detection results
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConflictReport {
    pub conflicts: Vec<Conflict>,
    pub has_conflicts: bool,
    pub total_conflicts: usize,
}

/// Transform JSON string to CIF (Common Intermediate Format)
///
/// # Arguments
/// * `source_json` - JSON string to transform
/// * `schema_json` - Schema definition JSON string
/// * `format_id` - Format identifier (e.g., "format_a", "format_b")
///
/// # Returns
/// Transformed JSON string in CIF format
pub fn transform_to_cif_string(
    source_json: String,
    schema_json: String,
    format_id: String,
) -> Result<String, String> {
    let source: Value =
        serde_json::from_str(&source_json).map_err(|e| format!("Invalid source JSON: {}", e))?;

    let schema: Value =
        serde_json::from_str(&schema_json).map_err(|e| format!("Invalid schema JSON: {}", e))?;

    let cif = transform_to_cif(&source, &schema, &format_id).map_err(|e| e.to_string())?;

    serde_json::to_string_pretty(&cif).map_err(|e| format!("Failed to serialize CIF: {}", e))
}

/// Compare two CIF JSON strings and return structured conflict report
///
/// # Arguments
/// * `cif_a` - First CIF JSON string
/// * `cif_b` - Second CIF JSON string
///
/// # Returns
/// JSON string containing conflict report
pub fn compare_json_string(cif_a: String, cif_b: String) -> Result<String, String> {
    let json_a: Value =
        serde_json::from_str(&cif_a).map_err(|e| format!("Invalid CIF A JSON: {}", e))?;

    let json_b: Value =
        serde_json::from_str(&cif_b).map_err(|e| format!("Invalid CIF B JSON: {}", e))?;

    let diffs = compare_json(&json_a, &json_b);

    let conflicts: Vec<Conflict> = diffs
        .into_iter()
        .map(|(path, (old_val, new_val))| Conflict {
            path,
            old_value: format!("{:?}", old_val),
            new_value: format!("{:?}", new_val),
        })
        .collect();

    let report = ConflictReport {
        has_conflicts: !conflicts.is_empty(),
        total_conflicts: conflicts.len(),
        conflicts,
    };

    serde_json::to_string_pretty(&report)
        .map_err(|e| format!("Failed to serialize conflict report: {}", e))
}
