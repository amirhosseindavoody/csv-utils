mod cli;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "csv", version, about = "High-performance CSV CLI + TUI explorer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print per-column row/null/max_width stats
    Stats {
        file: String,
    },
    /// Print distinct value combinations as JSON objects
    Unique {
        file: String,
        columns: String,
        #[arg(default_value_t = 50)]
        limit: usize,
    },
    /// Print rows as JSON objects
    Json {
        file: String,
        #[arg(default_value_t = 20)]
        limit: usize,
    },
    /// Print rows matching a filter expression as JSON objects
    Filter {
        file: String,
        expr: String,
        #[arg(default_value_t = 50)]
        limit: usize,
    },
    /// Launch the interactive table explorer
    Tui {
        file: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Stats { file }) => cli::run_stats(&file),
        Some(Commands::Unique { file, columns, limit }) => cli::run_unique(&file, &columns, limit),
        Some(Commands::Json { file, limit }) => cli::run_json(&file, limit),
        Some(Commands::Filter { file, expr, limit }) => cli::run_filter(&file, &expr, limit),
        Some(Commands::Tui { file }) => tui::run(file.as_deref()),
        None => tui::run(None),
    }
}
