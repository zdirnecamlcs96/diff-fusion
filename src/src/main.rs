use clap::Parser;
use diff_fusion::drivers::cli::{Cli, Commands};
use colored::*;
use diff_fusion::{compare_json, transform_to_cif};
use serde_json::Value;
use std::{error::Error, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Diff {
            a,
            b,
            schema,
            format_a,
            format_b,
        } => {
            let json_a: Value = serde_json::from_str(&fs::read_to_string(a)?)?;
            let json_b: Value = serde_json::from_str(&fs::read_to_string(b)?)?;

            let schema_doc: Value = serde_json::from_str(&fs::read_to_string(schema)?)?;

            // Transform both JSONs to Common Intermediate Format (CIF)
            let cif_a = transform_to_cif(&json_a, &schema_doc, &format_a)?;
            let cif_b = transform_to_cif(&json_b, &schema_doc, &format_b)?;

            println!("{}", "Transformed to CIF:".cyan());
            println!("{}", "CIF A:".dimmed());
            println!("{}\n", serde_json::to_string_pretty(&cif_a)?);
            println!("{}", "CIF B:".dimmed());
            println!("{}\n", serde_json::to_string_pretty(&cif_b)?);

            let diff = compare_json(&cif_a, &cif_b);

            if diff.is_empty() {
                println!("{}", "✓ No differences found.".green().bold());
            } else {
                println!("{}", "✗ Differences found:".yellow().bold());
                for (path, (old, new)) in diff {
                    println!(
                        "  {}: {} {} {}",
                        path.blue(),
                        format!("{:?}", old).red(),
                        "→".dimmed(),
                        format!("{:?}", new).green()
                    );
                }
            }
        }
    }

    Ok(())
}
