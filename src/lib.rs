//! # diff-fusion
//!
//! **A library for detecting conflicts between different JSON formats.**
//!
//! Think `git diff` for JSON data across multiple systems. Transform to a
//! common format (CIF), then compare and report differences.
//!
//! ## What This Library Does
//!
//! ```text
//! System A JSON → CIF ← System B JSON
//!                  ↓
//!          Compare & Report
//!                  ↓
//!         "Field X differs"
//!                  ↓
//!          You Resolve It
//! ```
//!
//! **Like `git diff`:**
//! - ✅ Shows what changed
//! - ✅ Detects conflicts
//! - ❌ Does NOT merge automatically
//! - ❌ Does NOT write changes back
//!
//! ## Quick Start
//!
//! ```rust
//! use diff_fusion::DiffFusion;
//! use serde_json::json;
//!
//! // 1. Define your schema
//! let schema = json!({
//!     "cif_schema": {
//!         "product_id": {"type": "string", "required": true},
//!         "price": {"type": "number", "required": true}
//!     },
//!     "transformations": {
//!         "salesforce": {
//!             "product_id": {"source_path": "Id", "type": "string"},
//!             "price": {"source_path": "Price__c", "type": "number"}
//!         },
//!         "shopify": {
//!             "product_id": {"source_path": "id", "type": "string"},
//!             "price": {"source_path": "variants.0.price", "type": "number"}
//!         }
//!     }
//! });
//!
//! // 2. Create facade
//! let diff_fusion = DiffFusion::new(schema);
//!
//! // 3. Transform and compare
//! let salesforce_data = json!({"Id": "SF-001", "Price__c": 29.99});
//! let shopify_data = json!({"id": "SH-001", "variants": [{"price": 34.99}]});
//!
//! let report = diff_fusion.transform_and_compare(
//!     &salesforce_data, "salesforce",
//!     &shopify_data, "shopify"
//! ).unwrap();
//!
//! // 4. You decide what to do with conflicts
//! for conflict in report.conflicts {
//!     println!("{} differs: {} vs {}",
//!         conflict.path, conflict.old_value, conflict.new_value);
//!     // Your code resolves the conflict
//! }
//! ```
//!
//! ## Core Concepts
//!
//! - **CIF (Common Intermediate Format)**: A unified schema that different formats transform into
//! - **Transformation**: Converting format-specific JSON to CIF
//! - **Comparison**: Detecting differences between two CIF objects
//! - **Conflict Report**: A list of fields that differ
//!
//! ## What's NOT Included
//!
//! This library does NOT:
//! - Merge data automatically
//! - Write changes back to systems
//! - Implement retry logic
//! - Handle transactions
//! - Manage sync orchestration
//!
//! Those are concerns for a sync engine that you build on top of this library.
//!
//! ## Name Explanation
//!
//! "diff-fusion" = **fusion of formats for diffing**, not fusion/merging of data values.

// Internal modules (implementation details)
mod compare;
mod transform;

// Public modules (user-facing API)
pub mod facade;
pub mod types;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================
// PRIMARY USER-FACING API (Facade Layer)
// ============================================
// Users should primarily use these high-level types
pub use facade::{DiffFusion, DiffFusionBuilder};

// ============================================
// SECONDARY API (For Advanced Users)
// ============================================
// Export types and traits for users who need them
pub use types::{CifFieldDefinition, CifSchema, CifType, ConflictStrategy, FieldTransformation};

// ============================================
// LEGACY/UTILITY API (For Backward Compatibility)
// ============================================
// Pure functions - still available but not the primary interface
pub use compare::compare_json;
pub use transform::{Transformer, transform_to_cif};

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
