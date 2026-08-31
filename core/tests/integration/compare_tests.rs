use diff_fusion::compare_json;
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

