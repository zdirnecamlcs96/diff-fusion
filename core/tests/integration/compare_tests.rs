use diff_fusion::{compare_json, compare_json_string, ConflictReport};
use serde_json::json;

#[test]
fn test_compare_no_differences() {
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
fn test_compare_with_differences() {
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
fn test_compare_numeric_equality() {
    let json_a = json!({"price": 5});
    let json_b = json!({"price": 5.0});

    let diffs = compare_json(&json_a, &json_b);
    assert!(diffs.is_empty(), "Numbers 5 and 5.0 should be equal");
}

#[test]
fn test_compare_nested_objects() {
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
fn test_compare_string_api() {
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
fn test_compare_string_invalid_json() {
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

    assert!(report.has_conflicts);
    assert_eq!(report.total_conflicts, 2);
    assert_eq!(report.conflicts.len(), 2);

    let paths: Vec<&str> = report.conflicts.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"b"));
    assert!(paths.contains(&"c"));
}
