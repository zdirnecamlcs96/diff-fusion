use clap::{Parser, Subcommand};

/// diffusion — JSON diff and transformation tool with Common Intermediate Format
#[derive(Parser)]
#[command(name = "diffusion", about = "A JSON diff and transformer CLI tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compare two JSON files
    Diff {
        /// First JSON file path
        a: String,
        /// Second JSON file path
        b: String,
        /// Schema file defining CIF (Common Intermediate Format)
        #[arg(short, long)]
        schema: String,
        /// Format identifier for file A (e.g., "format_a")
        #[arg(long, default_value = "format_a")]
        format_a: String,
        /// Format identifier for file B (e.g., "format_b")
        #[arg(long, default_value = "format_b")]
        format_b: String,
    },
}
