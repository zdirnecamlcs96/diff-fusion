use diff_fusion::{
    ConflictReport, compare_json, compare_json_string, transform_to_cif, transform_to_cif_string,
};
use serde_json::{Value, json};

#[test]
fn test_transform_to_cif_basic() {
    let source = json!({
        "name": "Widget",
        "price": 19.99
    });

    let schema = json!({
        "cif_schema": {
            "product_name": {"type": "string", "required": true},
            "product_price": {"type": "number", "required": true}
        },
        "transformations": {
            "format_a": {
                "product_name": {"source_path": "name", "type": "string"},
                "product_price": {"source_path": "price", "type": "number"}
            }
        }
    });

    let result = transform_to_cif(&source, &schema, "format_a");
    assert!(result.is_ok());

    let cif = result.unwrap();
    assert_eq!(cif["product_name"], "Widget");
    assert_eq!(cif["product_price"], 19.99);
}

#[test]
fn test_transform_to_cif_nested_path() {
    let source = json!({
        "product": {
            "details": {
                "name": "Gadget"
            }
        },
        "pricing": {
            "amount": 99.99
        }
    });

    let schema = json!({
        "cif_schema": {
            "product_name": {"type": "string", "required": true},
            "product_price": {"type": "number", "required": true}
        },
        "transformations": {
            "nested_format": {
                "product_name": {"source_path": "product.details.name", "type": "string"},
                "product_price": {"source_path": "pricing.amount", "type": "number"}
            }
        }
    });

    let result = transform_to_cif(&source, &schema, "nested_format");
    assert!(result.is_ok());

    let cif = result.unwrap();
    assert_eq!(cif["product_name"], "Gadget");
    assert_eq!(cif["product_price"], 99.99);
}

#[test]
fn test_transform_to_cif_type_conversion() {
    let source = json!({
        "id": "12345",
        "active": "true",
        "quantity": "100"
    });

    let schema = json!({
        "cif_schema": {
            "product_id": {"type": "string", "required": true},
            "is_active": {"type": "boolean", "required": true},
            "stock": {"type": "number", "required": true}
        },
        "transformations": {
            "format_b": {
                "product_id": {"source_path": "id", "type": "string"},
                "is_active": {"source_path": "active", "type": "boolean"},
                "stock": {"source_path": "quantity", "type": "number"}
            }
        }
    });

    let result = transform_to_cif(&source, &schema, "format_b");
    assert!(result.is_ok());

    let cif = result.unwrap();
    assert_eq!(cif["product_id"], "12345");
    assert_eq!(cif["is_active"], true);
    assert_eq!(cif["stock"], 100.0); // Numbers are stored as f64
}

#[test]
fn test_transform_to_cif_missing_required_field() {
    let source = json!({
        "name": "Widget"
        // Missing price
    });

    let schema = json!({
        "cif_schema": {
            "product_name": {"type": "string", "required": true},
            "product_price": {"type": "number", "required": true}
        },
        "transformations": {
            "format_a": {
                "product_name": {"source_path": "name", "type": "string"},
                "product_price": {"source_path": "price", "type": "number"}
            }
        }
    });

    let result = transform_to_cif(&source, &schema, "format_a");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Required field 'product_price'")
    );
}

#[test]
fn test_transform_to_cif_invalid_format_id() {
    let source = json!({"name": "Widget"});
    let schema = json!({
        "cif_schema": {},
        "transformations": {
            "format_a": {}
        }
    });

    let result = transform_to_cif(&source, &schema, "nonexistent_format");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Format 'nonexistent_format' not found")
    );
}

#[test]
fn test_compare_json_no_differences() {
    let json_a = json!({
        "product_name": "Widget",
        "product_price": 19.99
    });

    let json_b = json!({
        "product_name": "Widget",
        "product_price": 19.99
    });

    let diffs = compare_json(&json_a, &json_b);
    assert!(diffs.is_empty());
}

#[test]
fn test_compare_json_with_differences() {
    let json_a = json!({
        "product_name": "Widget",
        "product_price": 19.99,
        "stock": 100
    });

    let json_b = json!({
        "product_name": "Gadget",
        "product_price": 19.99,
        "stock": 95
    });

    let diffs = compare_json(&json_a, &json_b);
    assert_eq!(diffs.len(), 2);

    let paths: Vec<&str> = diffs.iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains(&"product_name"));
    assert!(paths.contains(&"stock"));
}

#[test]
fn test_compare_json_numeric_equality() {
    let json_a = json!({"price": 5});
    let json_b = json!({"price": 5.0});

    let diffs = compare_json(&json_a, &json_b);
    assert!(diffs.is_empty(), "Numbers 5 and 5.0 should be equal");
}

#[test]
fn test_compare_json_nested_objects() {
    let json_a = json!({
        "product": {
            "name": "Widget",
            "price": 19.99
        }
    });

    let json_b = json!({
        "product": {
            "name": "Widget",
            "price": 24.99
        }
    });

    let diffs = compare_json(&json_a, &json_b);
    assert_eq!(diffs.len(), 1);

    let paths: Vec<&str> = diffs.iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains(&"product.price"));
}

#[test]
fn test_transform_to_cif_string_api() {
    let source_json = r#"{"name": "Widget", "price": 19.99}"#.to_string();
    let schema_json = r#"{
        "cif_schema": {
            "product_name": {"type": "string", "required": true},
            "product_price": {"type": "number", "required": true}
        },
        "transformations": {
            "format_a": {
                "product_name": {"source_path": "name", "type": "string"},
                "product_price": {"source_path": "price", "type": "number"}
            }
        }
    }"#
    .to_string();

    let result = transform_to_cif_string(source_json, schema_json, "format_a".to_string());
    assert!(result.is_ok());

    let cif_json = result.unwrap();
    let cif: Value = serde_json::from_str(&cif_json).unwrap();
    assert_eq!(cif["product_name"], "Widget");
    assert_eq!(cif["product_price"], 19.99);
}

#[test]
fn test_compare_json_string_api() {
    let cif_a = r#"{"product_name": "Widget", "product_price": 19.99}"#.to_string();
    let cif_b = r#"{"product_name": "Widget", "product_price": 24.99}"#.to_string();

    let result = compare_json_string(cif_a, cif_b);
    assert!(result.is_ok());

    let report_json = result.unwrap();
    let report: ConflictReport = serde_json::from_str(&report_json).unwrap();

    assert!(report.has_conflicts);
    assert_eq!(report.total_conflicts, 1);
    assert_eq!(report.conflicts[0].path, "product_price");
}

#[test]
fn test_transform_to_cif_string_invalid_json() {
    let source_json = "invalid json".to_string();
    let schema_json = "{}".to_string();

    let result = transform_to_cif_string(source_json, schema_json, "format_a".to_string());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid source JSON"));
}

#[test]
fn test_compare_json_string_invalid_json() {
    let cif_a = "invalid json".to_string();
    let cif_b = "{}".to_string();

    let result = compare_json_string(cif_a, cif_b);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid CIF A JSON"));
}

#[test]
fn test_conflict_report_structure() {
    let cif_a = r#"{"a": 1, "b": 2, "c": 3}"#.to_string();
    let cif_b = r#"{"a": 1, "b": 5, "c": 6}"#.to_string();

    let result = compare_json_string(cif_a, cif_b);
    assert!(result.is_ok());

    let report: ConflictReport = serde_json::from_str(&result.unwrap()).unwrap();

    assert_eq!(report.has_conflicts, true);
    assert_eq!(report.total_conflicts, 2);
    assert_eq!(report.conflicts.len(), 2);

    // Check individual conflicts
    let paths: Vec<&str> = report.conflicts.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"b"));
    assert!(paths.contains(&"c"));
}

#[test]
fn test_end_to_end_transform_and_compare() {
    // Simulate two different system formats
    let system_a = json!({"id": "P123", "stock": 100});
    let system_b = json!({"product_id": "P123", "inventory": 95});

    let schema = json!({
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
    });

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
