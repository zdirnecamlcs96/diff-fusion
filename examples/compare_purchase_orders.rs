use diff_fusion::DiffFusion;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Purchase Order Comparison using DiffFusion ===\n");

    // 1. Load the schema
    let schema_str = fs::read_to_string("po_simple_schema.json")?;
    let schema: serde_json::Value = serde_json::from_str(&schema_str)?;

    // 2. Load the two PO documents
    let transformed_content = fs::read_to_string("test_data/transformed-po-1-fixed.json")?;
    let exists_content = fs::read_to_string("test_data/exists-po-1-fixed.json")?;

    let transformed_po: serde_json::Value = serde_json::from_str(&transformed_content)?;
    let existing_po: serde_json::Value = serde_json::from_str(&exists_content)?;

    // 3. Create DiffFusion instance
    let diff_fusion = DiffFusion::new(schema);

    // 4. Transform both documents to CIF
    println!("🔄 Transforming documents to CIF...\n");

    let cif_transformed = diff_fusion.transform(&transformed_po, "transformed_po")?;
    println!("✅ Transformed PO converted to CIF");

    let cif_existing = diff_fusion.transform(&existing_po, "existing_po")?;
    println!("✅ Existing PO converted to CIF\n");

    // 5. Compare the CIF documents
    println!("⚖️  Comparing Purchase Orders...\n");
    let report = diff_fusion.compare(&cif_transformed, &cif_existing);

    // 6. Display results
    if report.has_conflicts {
        println!(
            "⚠️  CONFLICTS DETECTED: {} differences found\n",
            report.total_conflicts
        );
        println!(
            "{:<25} {:<30} {:<30}",
            "Field", "Transformed PO", "Existing PO"
        );
        println!("{}", "=".repeat(85));

        for conflict in &report.conflicts {
            println!(
                "{:<25} {:<30} {:<30}",
                conflict.path,
                truncate(&conflict.old_value, 28),
                truncate(&conflict.new_value, 28)
            );
        }

        println!("\n📋 Summary:");
        println!("  - Total conflicts: {}", report.total_conflicts);
        println!("  - Fields checked: {}", get_field_count(&cif_transformed));

        // Categorize conflicts
        let critical_fields = ["po_status", "po_seq_number", "supplier_id"];
        let critical_conflicts: Vec<_> = report
            .conflicts
            .iter()
            .filter(|c| critical_fields.contains(&c.path.as_str()))
            .collect();

        if !critical_conflicts.is_empty() {
            println!("\n🚨 CRITICAL CONFLICTS:");
            for conflict in critical_conflicts {
                println!(
                    "  - {}: {} → {}",
                    conflict.path, conflict.old_value, conflict.new_value
                );
            }
        }
    } else {
        println!("✅ NO CONFLICTS - Documents are identical (in tracked fields)\n");
        println!("Both POs have:");
        println!("  - PO ID: {}", cif_transformed["po_id"]);
        println!("  - Status: {}", cif_transformed["po_status"]);
        println!("  - Supplier: {}", cif_transformed["supplier_name"]);
        println!("  - Seq #: {}", cif_transformed["po_seq_number"]);
    }

    println!("\n📝 Note: This comparison covers header fields only.");
    println!("   For item-level comparison, you'll need to implement custom logic.");

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn get_field_count(json: &serde_json::Value) -> usize {
    match json {
        serde_json::Value::Object(map) => map.len(),
        _ => 0,
    }
}
