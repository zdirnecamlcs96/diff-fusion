// Example: Using the DiffFusion facade (Simple, user-friendly API)
//
// This shows how library users interact with diff-fusion without
// needing to understand the internal implementation.

use diff_fusion::DiffFusion;
use serde_json::json;

fn main() {
    println!("🎯 diff-fusion Facade API Examples\n");
    println!("═══════════════════════════════════════════════════════════\n");

    // ============================================
    // Step 1: Create DiffFusion with your schema
    // ============================================
    println!("📋 Step 1: Initialize with schema\n");

    let schema = json!({
        "cif_schema": {
            "product_id": {"type": "string", "required": true},
            "product_name": {"type": "string", "required": true},
            "price": {"type": "number", "required": true},
            "in_stock": {"type": "boolean", "required": false}
        },
        "transformations": {
            // Salesforce format
            "salesforce": {
                "product_id": {"source_path": "Id", "type": "string"},
                "product_name": {"source_path": "Name", "type": "string"},
                "price": {"source_path": "Price__c", "type": "number"},
                "in_stock": {"source_path": "Stock_Available__c", "type": "boolean"}
            },
            // Shopify format
            "shopify": {
                "product_id": {"source_path": "id", "type": "string"},
                "product_name": {"source_path": "title", "type": "string"},
                "price": {"source_path": "variants.0.price", "type": "number"},
                "in_stock": {"source_path": "available", "type": "boolean"}
            }
        }
    });

    // Create the facade - this is your main interface
    let diff_fusion = DiffFusion::new(schema);
    println!("✅ DiffFusion initialized\n");

    // ============================================
    // Step 2: Transform data from different sources
    // ============================================
    println!("🔄 Step 2: Transform data to CIF\n");

    // Data from Salesforce
    let salesforce_data = json!({
        "Id": "SF-001",
        "Name": "Wireless Mouse",
        "Price__c": 29.99,
        "Stock_Available__c": true
    });

    let cif_from_salesforce = diff_fusion
        .transform(&salesforce_data, "salesforce")
        .expect("Transform failed");

    println!("📤 Salesforce → CIF:");
    println!(
        "{}\n",
        serde_json::to_string_pretty(&cif_from_salesforce).unwrap()
    );

    // Data from Shopify
    let shopify_data = json!({
        "id": "SH-001",
        "title": "Wireless Mouse",
        "variants": [
            {"price": 34.99}
        ],
        "available": true
    });

    let cif_from_shopify = diff_fusion
        .transform(&shopify_data, "shopify")
        .expect("Transform failed");

    println!("📤 Shopify → CIF:");
    println!(
        "{}\n",
        serde_json::to_string_pretty(&cif_from_shopify).unwrap()
    );

    // ============================================
    // Step 3: Compare and detect conflicts
    // ============================================
    println!("⚖️  Step 3: Detect conflicts between sources\n");

    let report = diff_fusion.compare(&cif_from_salesforce, &cif_from_shopify);

    if report.has_conflicts {
        println!(
            "⚠️  Conflicts detected! Total: {}\n",
            report.total_conflicts
        );
        for conflict in &report.conflicts {
            println!("  • Field '{}' differs:", conflict.path);
            println!("    Salesforce: {}", conflict.old_value);
            println!("    Shopify:    {}", conflict.new_value);
        }
    } else {
        println!("✅ No conflicts - data is in sync!");
    }
    println!();

    // ============================================
    // Step 4: One-liner workflow
    // ============================================
    println!("⚡ Step 4: Transform and compare in one call\n");

    let quick_report = diff_fusion
        .transform_and_compare(&salesforce_data, "salesforce", &shopify_data, "shopify")
        .expect("Workflow failed");

    println!("📊 Quick Report:");
    println!("   Conflicts: {}", quick_report.total_conflicts);
    println!("   Has conflicts: {}\n", quick_report.has_conflicts);

    // ============================================
    // Step 5: Validation
    // ============================================
    println!("✓ Step 5: Validate CIF data\n");

    match diff_fusion.validate_cif(&cif_from_salesforce) {
        Ok(_) => println!("✅ Salesforce CIF is valid"),
        Err(errors) => {
            println!("❌ Validation errors:");
            for error in errors {
                println!("   - {}", error);
            }
        }
    }

    match diff_fusion.validate_cif(&cif_from_shopify) {
        Ok(_) => println!("✅ Shopify CIF is valid"),
        Err(errors) => {
            println!("❌ Validation errors:");
            for error in errors {
                println!("   - {}", error);
            }
        }
    }

    println!("\n═══════════════════════════════════════════════════════════");
    println!("\n💡 Key Benefits:");
    println!("   • Simple API - no need to understand internals");
    println!("   • Type-safe - schema-driven transformations");
    println!("   • One instance handles all formats");
    println!("   • Built-in validation");
    println!("   • Clear conflict detection");
}
