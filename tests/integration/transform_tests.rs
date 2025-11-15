use diff_fusion::{transform_to_cif, transform_to_cif_string};
use serde_json::{json, Value};

// Include test helpers
fn basic_product_schema() -> Value {
    json!({
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
    })
}

fn nested_schema() -> Value {
    json!({
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
    })
}

fn type_conversion_schema() -> Value {
    json!({
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
    })
}

#[test]
fn test_transform_basic() {
    let source = json!({
        "name": "Widget",
        "price": 19.99
    });

    let schema = basic_product_schema();
    let result = transform_to_cif(&source, &schema, "format_a");

    assert!(result.is_ok());
    let cif = result.unwrap();
    assert_eq!(cif["product_name"], "Widget");
    assert_eq!(cif["product_price"], 19.99);
}

#[test]
fn test_transform_nested_path() {
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

    let schema = nested_schema();
    let result = transform_to_cif(&source, &schema, "nested_format");

    assert!(result.is_ok());
    let cif = result.unwrap();
    assert_eq!(cif["product_name"], "Gadget");
    assert_eq!(cif["product_price"], 99.99);
}

#[test]
fn test_transform_type_conversion() {
    let source = json!({
        "id": "12345",
        "active": "true",
        "quantity": "100"
    });

    let schema = type_conversion_schema();
    let result = transform_to_cif(&source, &schema, "format_b");

    assert!(result.is_ok());
    let cif = result.unwrap();
    assert_eq!(cif["product_id"], "12345");
    assert_eq!(cif["is_active"], true);
    assert_eq!(cif["stock"], 100.0);
}

#[test]
fn test_transform_missing_required_field() {
    let source = json!({
        "name": "Widget"
        // Missing price
    });

    let schema = basic_product_schema();
    let result = transform_to_cif(&source, &schema, "format_a");

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Required field 'product_price'"));
}

#[test]
fn test_transform_invalid_format_id() {
    let source = json!({"name": "Widget"});
    let schema = json!({
        "cif_schema": {},
        "transformations": {
            "format_a": {}
        }
    });

    let result = transform_to_cif(&source, &schema, "nonexistent_format");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Format 'nonexistent_format' not found"));
}

#[test]
fn test_transform_string_api() {
    let source_json = r#"{"name": "Widget", "price": 19.99}"#.to_string();
    let schema_json = serde_json::to_string(&basic_product_schema()).unwrap();

    let result = transform_to_cif_string(source_json, schema_json, "format_a".to_string());
    assert!(result.is_ok());

    let cif_json = result.unwrap();
    let cif: Value = serde_json::from_str(&cif_json).unwrap();
    assert_eq!(cif["product_name"], "Widget");
    assert_eq!(cif["product_price"], 19.99);
}

#[test]
fn test_transform_string_invalid_json() {
    let source_json = "invalid json".to_string();
    let schema_json = "{}".to_string();

    let result = transform_to_cif_string(source_json, schema_json, "format_a".to_string());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid source JSON"));
}
