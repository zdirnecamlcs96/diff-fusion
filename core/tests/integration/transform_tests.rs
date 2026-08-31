use diff_fusion::transform_to_cif;
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
fn test_transform_array_of_objects_with_element_schema() {
    // Cross-system array: System A's source carries `externalId` per line,
    // System B's source carries `internalId`. The CIF element declares
    // both as anchors so downstream SetByKey policies can rehome rows
    // across renames.
    let schema = json!({
        "cif_schema": {
            "items": {
                "type": "array",
                "required": true,
                "element": {
                    "externalId": {"type": "string", "anchor": "a"},
                    "internalId": {"type": "string", "anchor": "b"},
                    "sku": {"type": "string", "required": true},
                    "qty": {"type": "number"}
                }
            }
        },
        "transformations": {
            "erp": {
                "items": {
                    "source_path": "lineItems",
                    "type": "array",
                    "element": {
                        "externalId": {"source_path": "extId", "type": "string"},
                        "sku": {"source_path": "sku", "type": "string"},
                        "qty": {"source_path": "quantity", "type": "number"}
                    }
                }
            }
        }
    });

    let source = json!({
        "lineItems": [
            {"extId": "A-1", "sku": "SKU-X", "quantity": 3},
            {"extId": "A-2", "sku": "SKU-Y", "quantity": 5}
        ]
    });

    let cif = transform_to_cif(&source, &schema, "erp").expect("transform");
    let items = cif["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["externalId"], json!("A-1"));
    assert_eq!(items[0]["sku"], json!("SKU-X"));
    assert_eq!(items[0]["qty"], json!(3));
    // internalId is declared on the CIF element but has no source path in
    // the `erp` transformation, so it should simply be absent (populated
    // later when B's row merges in via SetByKey Union).
    assert!(items[0].get("internalId").is_none());
}

#[test]
fn test_transform_array_element_required_field_missing_errors() {
    let schema = json!({
        "cif_schema": {
            "items": {
                "type": "array",
                "element": {
                    "sku": {"type": "string", "required": true}
                }
            }
        },
        "transformations": {
            "erp": {
                "items": {
                    "source_path": "lineItems",
                    "type": "array",
                    "element": {
                        "sku": {"source_path": "sku", "type": "string"}
                    }
                }
            }
        }
    });

    let source = json!({
        "lineItems": [
            {"sku": "SKU-X"},
            {}  // missing required sku
        ]
    });

    let err = transform_to_cif(&source, &schema, "erp").unwrap_err();
    assert!(
        err.to_string().contains("required element field 'items.sku'"),
        "unexpected error: {err}"
    );
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

// Ported from sdk/typescript/tests/unit/application/transform.test.ts
// ("edge cases not covered by Rust suite") — object `children` composition.

#[test]
fn test_transform_object_children_build_nested_cif_object() {
    let schema = json!({
        "cif_schema": {
            "supplier": {
                "type": "object",
                "required": true,
                "children": {
                    "name": {"type": "string", "required": true},
                    "email": {"type": "string"}
                }
            }
        },
        "transformations": {
            "f": {
                "supplier": {
                    "source_path": "vendor",
                    "type": "object",
                    "children": {
                        "name": {"source_path": "companyName", "type": "string"},
                        "email": {"source_path": "contact.email", "type": "string"}
                    }
                }
            }
        }
    });
    let source = json!({
        "vendor": {"companyName": "Acme", "contact": {"email": "a@acme.io"}}
    });

    let cif = transform_to_cif(&source, &schema, "f").expect("transform");
    assert_eq!(cif, json!({"supplier": {"name": "Acme", "email": "a@acme.io"}}));
}

#[test]
fn test_transform_object_children_required_child_missing_errors() {
    let schema = json!({
        "cif_schema": {
            "supplier": {
                "type": "object",
                "children": {"name": {"type": "string", "required": true}}
            }
        },
        "transformations": {
            "f": {
                "supplier": {
                    "source_path": "vendor",
                    "type": "object",
                    "children": {"name": {"source_path": "companyName", "type": "string"}}
                }
            }
        }
    });
    let source = json!({"vendor": {}});

    let err = transform_to_cif(&source, &schema, "f").unwrap_err();
    assert!(
        err.to_string().contains("required children field 'supplier.name'"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_transform_top_level_source_path_dot_collects_root_fields() {
    let schema = json!({
        "cif_schema": {
            "net_suite": {
                "type": "object",
                "children": {"id": {"type": "string"}}
            }
        },
        "transformations": {
            "f": {
                "net_suite": {
                    "source_path": ".",
                    "type": "object",
                    "children": {"id": {"source_path": "id", "type": "string"}}
                }
            }
        }
    });
    let source = json!({"id": "42", "other": true});

    let cif = transform_to_cif(&source, &schema, "f").expect("transform");
    assert_eq!(cif, json!({"net_suite": {"id": "42"}}));
}

#[test]
fn test_transform_object_child_source_path_dot_takes_parent_value() {
    let schema = json!({
        "cif_schema": {
            "meta": {
                "type": "object",
                "children": {"raw": {"type": "string"}}
            }
        },
        "transformations": {
            "f": {
                "meta": {
                    "source_path": "tag",
                    "type": "object",
                    "children": {"raw": {"source_path": ".", "type": "string"}}
                }
            }
        }
    });
    let source = json!({"tag": "urgent"});

    let cif = transform_to_cif(&source, &schema, "f").expect("transform");
    assert_eq!(cif, json!({"meta": {"raw": "urgent"}}));
}

#[test]
fn test_transform_objects_in_array_elements_and_arrays_in_objects_compose() {
    let schema = json!({
        "cif_schema": {
            "items": {
                "type": "array",
                "element": {
                    "sku": {"type": "string"},
                    "dimensions": {
                        "type": "object",
                        "children": {
                            "width": {"type": "number"},
                            "height": {"type": "number"}
                        }
                    }
                }
            },
            "supplier": {
                "type": "object",
                "children": {
                    "name": {"type": "string"},
                    "addresses": {
                        "type": "array",
                        "element": {"city": {"type": "string"}}
                    }
                }
            }
        },
        "transformations": {
            "f": {
                "items": {
                    "source_path": "lines",
                    "type": "array",
                    "element": {
                        "sku": {"source_path": "sku", "type": "string"},
                        "dimensions": {
                            "source_path": "dims",
                            "type": "object",
                            "children": {
                                "width": {"source_path": "w", "type": "number"},
                                "height": {"source_path": "h", "type": "number"}
                            }
                        }
                    }
                },
                "supplier": {
                    "source_path": "vendor",
                    "type": "object",
                    "children": {
                        "name": {"source_path": "name", "type": "string"},
                        "addresses": {
                            "source_path": "addrs",
                            "type": "array",
                            "element": {"city": {"source_path": "city", "type": "string"}}
                        }
                    }
                }
            }
        }
    });
    let source = json!({
        "lines": [{"sku": "X", "dims": {"w": 2, "h": 3}}],
        "vendor": {"name": "Acme", "addrs": [{"city": "Oslo"}, {"city": "Bergen"}]}
    });

    let cif = transform_to_cif(&source, &schema, "f").expect("transform");
    assert_eq!(
        cif,
        json!({
            "items": [{"sku": "X", "dimensions": {"width": 2, "height": 3}}],
            "supplier": {"name": "Acme", "addresses": [{"city": "Oslo"}, {"city": "Bergen"}]}
        })
    );
}

#[test]
fn test_transform_nested_arrays_of_objects_walked_recursively() {
    let schema = json!({
        "cif_schema": {
            "orders": {
                "type": "array",
                "element": {
                    "id": {"type": "string"},
                    "lines": {
                        "type": "array",
                        "element": {"sku": {"type": "string"}}
                    }
                }
            }
        },
        "transformations": {
            "f": {
                "orders": {
                    "source_path": "orders",
                    "type": "array",
                    "element": {
                        "id": {"source_path": "id", "type": "string"},
                        "lines": {
                            "source_path": "lines",
                            "type": "array",
                            "element": {"sku": {"source_path": "sku", "type": "string"}}
                        }
                    }
                }
            }
        }
    });
    let source = json!({
        "orders": [
            {"id": "o1", "lines": [{"sku": "a"}, {"sku": "b"}]},
            {"id": "o2", "lines": [{"sku": "c"}]}
        ]
    });

    let cif = transform_to_cif(&source, &schema, "f").expect("transform");
    assert_eq!(
        cif,
        json!({
            "orders": [
                {"id": "o1", "lines": [{"sku": "a"}, {"sku": "b"}]},
                {"id": "o2", "lines": [{"sku": "c"}]}
            ]
        })
    );
}
