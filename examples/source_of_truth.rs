// Example: Source of Truth and Conflict Resolution Strategies
//
// This demonstrates how to handle the question:
// "Who is the source of truth when syncing between systems?"

use diff_fusion::{
    DiffFusion,
    types::{CifFieldDefinition, ConflictStrategy},
};
use serde_json::json;

fn main() {
    println!("🎯 Source of Truth & Conflict Resolution\n");
    println!("═══════════════════════════════════════════════════════════\n");

    // ============================================
    // Pattern 1: Field-Level Source of Truth (RECOMMENDED)
    // ============================================
    println!("📋 Pattern 1: Field-Level Source of Truth\n");
    println!("Break down data by domain - each field has a clear owner\n");

    let schema_with_sources = json!({
        "cif_schema": {
            // Inventory system owns stock data
            "product_id": {
                "type": "string",
                "required": true,
                "source_of_truth": "inventory_system",
                "description": "Inventory generates unique IDs"
            },
            "stock_quantity": {
                "type": "number",
                "required": true,
                "source_of_truth": "inventory_system",
                "description": "Only inventory knows real stock levels"
            },

            // Pricing system owns pricing data
            "price": {
                "type": "number",
                "required": true,
                "source_of_truth": "pricing_system",
                "description": "Pricing team controls all prices"
            },
            "discount_percentage": {
                "type": "number",
                "required": false,
                "source_of_truth": "pricing_system",
                "description": "Promotions managed by pricing"
            },

            // Product catalog owns descriptive data
            "product_name": {
                "type": "string",
                "required": true,
                "source_of_truth": "product_catalog",
                "description": "Marketing controls product names"
            },
            "description": {
                "type": "string",
                "required": false,
                "source_of_truth": "product_catalog",
                "description": "Product descriptions from catalog"
            },

            // Customer system owns ratings
            "average_rating": {
                "type": "number",
                "required": false,
                "source_of_truth": "review_system",
                "description": "Calculated from customer reviews"
            }
        },
        "transformations": {
            "inventory_system": {
                "product_id": {"source_path": "sku", "type": "string"},
                "stock_quantity": {"source_path": "quantity_on_hand", "type": "number"},
                "price": {"source_path": "unit_price", "type": "number"},
                "discount_percentage": {"source_path": "discount", "type": "number"},
                "product_name": {"source_path": "item_name", "type": "string"},
                "description": {"source_path": "item_description", "type": "string"},
                "average_rating": {"source_path": "rating", "type": "number"}
            },
            "pricing_system": {
                "product_id": {"source_path": "product_code", "type": "string"},
                "stock_quantity": {"source_path": "available_stock", "type": "number"},
                "price": {"source_path": "current_price", "type": "number"},
                "discount_percentage": {"source_path": "promo_discount", "type": "number"},
                "product_name": {"source_path": "name", "type": "string"},
                "description": {"source_path": "desc", "type": "string"},
                "average_rating": {"source_path": "avg_stars", "type": "number"}
            }
        }
    });

    let diff_fusion = DiffFusion::new(schema_with_sources);

    // Simulate data from both systems
    let inventory_data = json!({
        "sku": "INV-001",
        "quantity_on_hand": 100,         // ← INVENTORY is source of truth
        "unit_price": 29.99,
        "discount": 10.0,
        "item_name": "Wireless Mouse",
        "item_description": "Old description",
        "rating": 4.2
    });

    let pricing_data = json!({
        "product_code": "INV-001",
        "available_stock": 95,           // Different, but inventory wins
        "current_price": 34.99,          // ← PRICING is source of truth
        "promo_discount": 15.0,          // ← PRICING is source of truth
        "name": "Premium Wireless Mouse", // Different, but catalog should win
        "desc": "Updated marketing copy",
        "avg_stars": 4.5
    });

    let cif_inventory = diff_fusion
        .transform(&inventory_data, "inventory_system")
        .unwrap();
    let cif_pricing = diff_fusion
        .transform(&pricing_data, "pricing_system")
        .unwrap();

    let report = diff_fusion.compare(&cif_inventory, &cif_pricing);

    println!("⚠️  Conflicts Detected: {}\n", report.total_conflicts);

    for conflict in &report.conflicts {
        println!("Field: {}", conflict.path);
        println!("  Inventory says: {}", conflict.old_value);
        println!("  Pricing says:   {}", conflict.new_value);

        // Show which system wins based on source_of_truth
        let winner = match conflict.path.as_str() {
            "stock_quantity" => "✅ Use Inventory (source of truth)",
            "price" | "discount_percentage" => "✅ Use Pricing (source of truth)",
            "product_name" | "description" => "✅ Use Catalog (source of truth)",
            "average_rating" => "✅ Use Review System (source of truth)",
            _ => "⚠️  Manual resolution needed",
        };
        println!("  Resolution: {}\n", winner);
    }

    // ============================================
    // Pattern 2: Conflict Resolution Strategies
    // ============================================
    println!("\n═══════════════════════════════════════════════════════════");
    println!("\n📋 Pattern 2: Conflict Strategies (When No Clear Owner)\n");

    // Example: Timestamp-based resolution
    println!("Strategy 1: Last-Write-Wins");
    let field_with_strategy = CifFieldDefinition::new("string")
        .with_conflict_strategy(ConflictStrategy::LastWriteWins)
        .with_description("Use most recent update (requires timestamp)");

    println!("  Field: {:?}", field_with_strategy.conflict_strategy);
    println!("  Use case: Collaborative editing (like Google Docs)\n");

    // Example: Business rule - always prefer external system
    println!("Strategy 2: Prefer External System");
    let prefer_external = CifFieldDefinition::new("number")
        .with_conflict_strategy(ConflictStrategy::PreferB)
        .with_description("Customer-facing system is always right");

    println!("  Field: {:?}", prefer_external.conflict_strategy);
    println!("  Use case: External API is authoritative\n");

    // Example: Use maximum value
    println!("Strategy 3: Use Maximum Value");
    let use_max = CifFieldDefinition::new("number")
        .with_conflict_strategy(ConflictStrategy::UseMax)
        .with_description("Use the higher price");

    println!("  Field: {:?}", use_max.conflict_strategy);
    println!("  Use case: Safety margin - use higher estimate\n");

    // ============================================
    // Pattern 3: Context Separation (Your Suggestion!)
    // ============================================
    println!("\n═══════════════════════════════════════════════════════════");
    println!("\n📋 Pattern 3: Context Separation (BEST PRACTICE)\n");
    println!("💡 Your insight: Break down complex objects by domain context\n");

    println!("❌ BAD: Single monolithic product object");
    println!("   {{");
    println!("     product_id, name, price, stock,");
    println!("     supplier_info, customer_reviews,");
    println!("     shipping_details, tax_info");
    println!("   }}");
    println!("   → Who owns this? Everyone and no one!\n");

    println!("✅ GOOD: Separate contexts with clear ownership");
    println!("   1. Product Core (Catalog owns)");
    println!("      {{ product_id, name, description, category }}");
    println!();
    println!("   2. Inventory Context (Inventory owns)");
    println!("      {{ product_id, stock_quantity, warehouse_location }}");
    println!();
    println!("   3. Pricing Context (Pricing owns)");
    println!("      {{ product_id, price, discount, currency }}");
    println!();
    println!("   4. Customer Context (Reviews own)");
    println!("      {{ product_id, rating, review_count, sentiment }}");
    println!();
    println!("   → Each context has ONE clear source of truth!");

    // ============================================
    // Summary
    // ============================================
    println!("\n═══════════════════════════════════════════════════════════");
    println!("\n💡 Key Takeaways:\n");
    println!("1. ✅ CIF makes direction irrelevant (A↔B becomes A→CIF←B)");
    println!("2. ✅ Field-level source of truth is most practical");
    println!("3. ✅ Break complex objects into domain contexts");
    println!("4. ✅ Use conflict strategies when ownership is unclear");
    println!("5. ✅ Document source of truth in schema (not code!)");

    println!("\n📚 Practical Example:");
    println!("   Inventory System ─→ CIF ←─ Shopify");
    println!("                       ↓");
    println!("   Resolution: Inventory owns stock,");
    println!("               Shopify owns pricing,");
    println!("               Catalog owns descriptions");

    println!("\n✨ Result: No more arguments about who's right!");
}
