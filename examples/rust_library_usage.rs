// Example: Using diff-fusion as a Rust library

use diff_fusion::{
    ConflictReport, compare_json, compare_json_string, transform_to_cif, transform_to_cif_string,
};
use serde_json::{Value, json};

fn main() {
    println!("🚀 diff-fusion Library Examples\n");

    // Example 1: Transform using Value API (idiomatic Rust)
    example_1_value_api();

    // Example 2: Transform using String API (FFI-compatible)
    example_2_string_api();

    // Example 3: Real-world use case
    example_3_real_world();
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
        Err(e) => println!("❌ Error: {}", e),
    }
}
