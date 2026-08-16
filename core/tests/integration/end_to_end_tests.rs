use diff_fusion::{compare_json, transform_to_cif};
use serde_json::json;

// Test helper
fn multi_system_schema() -> serde_json::Value {
    json!({
        "cif_schema": {
            "product_id": {"type": "string", "required": true},
            "quantity": {"type": "number", "required": true}
        },
        "transformations": {
            "system_a": {
                "product_id": {"source_path": "id", "type": "string"},
                "quantity": {"source_path": "stock", "type": "number"}
            },
            "system_b": {
                "product_id": {"source_path": "product_id", "type": "string"},
                "quantity": {"source_path": "inventory", "type": "number"}
            }
        }
    })
}

#[test]
fn test_end_to_end_transform_and_compare() {
    // Simulate two different system formats
    let system_a = json!({"id": "P123", "stock": 100});
    let system_b = json!({"product_id": "P123", "inventory": 95});

    let schema = multi_system_schema();

    // Transform both to CIF
    let cif_a = transform_to_cif(&system_a, &schema, "system_a").unwrap();
    let cif_b = transform_to_cif(&system_b, &schema, "system_b").unwrap();

    // Compare
    let diffs = compare_json(&cif_a, &cif_b);

    assert_eq!(diffs.len(), 1);

    let (path, (old_val, new_val)) = &diffs[0];
    assert_eq!(path, "quantity");
    assert_eq!(old_val, &json!(100));
    assert_eq!(new_val, &json!(95));
}
