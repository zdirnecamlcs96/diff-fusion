use serde_json::{Value, json};

/// Create a basic product schema for testing
pub fn basic_product_schema() -> Value {
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

/// Create a schema with nested paths
pub fn nested_schema() -> Value {
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

/// Create a schema for type conversion testing
pub fn type_conversion_schema() -> Value {
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

/// Create a multi-system schema for end-to-end testing
pub fn multi_system_schema() -> Value {
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
