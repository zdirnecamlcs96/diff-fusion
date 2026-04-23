// Example: Using diff-fusion as a Rust library

use diff_fusion::{
    ConflictReport, compare_json, compare_json_string, transform_to_cif, transform_to_cif_string,
    domain::types::{CifFieldDefinition, Schema},
};
use serde_json::json;

fn main() {
    println!("🚀 diff-fusion Library Examples\n");

    // Example 1: Transform using Value API (idiomatic Rust)
    example_1_value_api();

    // Example 2: Transform using String API (FFI-compatible)
    example_2_string_api();

    // Example 3: Real-world use case
    example_3_real_world();

    // Example 4: Builder pattern for CIF field definitions
    example_4_builder_pattern();

    // Example 5: Trait-based schema (compile-time safety)
    example_5_trait_schema();
}

fn example_1_value_api() {
    println!("📝 Example 1: Value API (Idiomatic Rust)\n");

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

    match transform_to_cif(&source, &schema, "format_a") {
        Ok(cif) => println!("✅ CIF: {}\n", serde_json::to_string_pretty(&cif).unwrap()),
        Err(e) => println!("❌ Error: {}\n", e),
    }
}

fn example_2_string_api() {
    println!("📝 Example 2: String API (FFI-compatible)\n");

    let source_json = r#"{"id": "P123", "stock": 100}"#.to_string();
    let schema_json = r#"{
        "cif_schema": {
            "product_id": {"type": "string", "required": true},
            "quantity": {"type": "number", "required": true}
        },
        "transformations": {
            "system_a": {
                "product_id": {"source_path": "id", "type": "string"},
                "quantity": {"source_path": "stock", "type": "number"}
            }
        }
    }"#
    .to_string();

    match transform_to_cif_string(source_json, schema_json, "system_a".to_string()) {
        Ok(cif_json) => println!("✅ CIF JSON:\n{}\n", cif_json),
        Err(e) => println!("❌ Error: {}\n", e),
    }
}

fn example_3_real_world() {
    println!("🌐 Example 3: Real-World API Sync\n");

    // Simulate database state (System A)
    let db_state = json!({
        "id": "P123",
        "stock": 50,
        "updated_at": "2025-11-04T10:00:00Z"
    });

    // Simulate external API response (System B)
    let external_data = json!({
        "product_id": "P123",
        "inventory_quantity": 45,
        "last_updated": "2025-11-04T10:30:00Z"
    });

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
                "quantity": {"source_path": "inventory_quantity", "type": "number"}
            }
        }
    });

    // Transform both to CIF
    let cif_a = transform_to_cif(&db_state, &schema, "system_a").unwrap();
    let cif_b = transform_to_cif(&external_data, &schema, "system_b").unwrap();

    // Compare
    let diffs = compare_json(&cif_a, &cif_b);

    if diffs.is_empty() {
        println!("✅ No conflicts - systems are in sync!\n");
    } else {
        println!("⚠️  Conflicts detected:");
        for (path, (old_val, new_val)) in diffs {
            println!("   {}: DB({:?}) vs External({:?})", path, old_val, new_val);

            // Business logic for conflict resolution
            if path == "quantity" {
                println!("   ✓ Resolution: Use external value (source of truth)");
            }
        }
        println!();
    }

    // Using string API for structured output
    let cif_a_str = serde_json::to_string(&cif_a).unwrap();
    let cif_b_str = serde_json::to_string(&cif_b).unwrap();

    match compare_json_string(cif_a_str, cif_b_str) {
        Ok(report_json) => {
            let report: ConflictReport = serde_json::from_str(&report_json).unwrap();
            println!("📊 Structured Conflict Report:");
            println!("   Has conflicts: {}", report.has_conflicts);
            println!("   Total conflicts: {}", report.total_conflicts);
            for conflict in report.conflicts {
                println!(
                    "   - {}: {} → {}",
                    conflict.path, conflict.old_value, conflict.new_value
                );
            }
        }
        Err(e) => println!("❌ Error: {}\n", e),
    }
}

fn example_4_builder_pattern() {
    println!("📝 Example 4: Builder Pattern for CIF Schema\n");

    // Demonstrate the ergonomic builder pattern for CifFieldDefinition
    let email_field = CifFieldDefinition::new("string")
        .required()
        .with_description("User's email address");

    let age_field = CifFieldDefinition::new("number")
        .optional()
        .with_description("User's age")
        .with_default(json!(0));

    let active_field = CifFieldDefinition::new("boolean")
        .required()
        .with_description("Whether user account is active");

    println!("✅ Email field definition:");
    println!("   Type: {}", email_field.field_type);
    println!("   Required: {}", email_field.required);
    println!("   Description: {:?}", email_field.description);
    println!();

    println!("✅ Age field definition:");
    println!("   Type: {}", age_field.field_type);
    println!("   Required: {}", age_field.required);
    println!("   Default: {:?}", age_field.default);
    println!();

    println!("✅ Active field definition:");
    println!("   Type: {}", active_field.field_type);
    println!("   Required: {}", active_field.required);
    println!();

    // Show validation
    match email_field.validate() {
        Ok(_) => println!("✅ Email field is valid"),
        Err(e) => println!("❌ Validation error: {}", e),
    }

    // Show type parsing
    if let Some(cif_type) = email_field.get_type() {
        println!("📝 Parsed CIF type: {}", cif_type);
        println!("   Is primitive? {}", cif_type.is_primitive());
    }
    println!();
}

fn example_5_trait_schema() {
    println!("🎯 Example 5: Trait-based Schema (Compile-time Safety)\n");

    // Define a Product schema using the CifSchema trait
    struct Product;
    impl Schema for Product {
        fn schema_name() -> &'static str {
            "product"
        }

        fn fields() -> Vec<(&'static str, CifFieldDefinition)> {
            vec![
                (
                    "product_id",
                    CifFieldDefinition::new("string")
                        .required()
                        .with_description("Unique product identifier"),
                ),
                (
                    "name",
                    CifFieldDefinition::new("string")
                        .required()
                        .with_description("Product name"),
                ),
                (
                    "price",
                    CifFieldDefinition::new("number")
                        .required()
                        .with_description("Product price in USD"),
                ),
                (
                    "in_stock",
                    CifFieldDefinition::new("boolean")
                        .optional()
                        .with_description("Stock availability")
                        .with_default(json!(true)),
                ),
            ]
        }
    }

    // Generate JSON schema from the trait
    let schema = Product::to_json_schema();
    println!("✅ Generated JSON Schema:");
    println!("{}\n", serde_json::to_string_pretty(&schema).unwrap());

    // Validate valid data
    let valid_product = json!({
        "product_id": "P123",
        "name": "Laptop",
        "price": 999.99,
        "in_stock": true
    });

    match Product::validate(&valid_product) {
        Ok(_) => println!("✅ Valid product data passed validation"),
        Err(errors) => println!("❌ Validation errors: {:?}", errors),
    }

    // Validate invalid data (missing required field)
    let invalid_product = json!({
        "product_id": "P456",
        "name": "Mouse"
        // Missing required 'price' field
    });

    match Product::validate(&invalid_product) {
        Ok(_) => println!("✅ Product validated"),
        Err(errors) => {
            println!("\n⚠️  Invalid product data (as expected):");
            for error in errors {
                println!("   - {}", error);
            }
        }
    }

    // Type mismatch validation
    let wrong_type = json!({
        "product_id": "P789",
        "name": "Keyboard",
        "price": "not a number"  // Wrong type!
    });

    match Product::validate(&wrong_type) {
        Ok(_) => println!("✅ Product validated"),
        Err(errors) => {
            println!("\n⚠️  Type mismatch detected:");
            for error in errors {
                println!("   - {}", error);
            }
        }
    }

    println!("\n💡 Trait-based schemas provide:");
    println!("   • Compile-time type safety");
    println!("   • IDE autocomplete support");
    println!("   • Automatic JSON schema generation");
    println!("   • Runtime validation");
    println!();
}
