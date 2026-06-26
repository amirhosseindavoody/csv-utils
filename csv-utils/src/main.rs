mod cli;
mod tui;
mod web;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "csv", version, about = "High-performance CSV CLI + TUI explorer")]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    /// Open FILE in the interactive table explorer (when no subcommand is given)
    file: Option<String>,
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
    /// Launch the interactive table explorer (alias for `csv [file]`)
    Tui {
        file: Option<String>,
    },
}

fn run_tui(file: Option<&str>) -> Result<()> {
    tui::run(file)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Stats { file }) => cli::run_stats(&file),
        Some(Commands::Unique { file, columns, limit }) => cli::run_unique(&file, &columns, limit),
        Some(Commands::Json { file, limit }) => cli::run_json(&file, limit),
        Some(Commands::Filter { file, expr, limit }) => cli::run_filter(&file, &expr, limit),
        Some(Commands::Tui { file }) => run_tui(file.as_deref()),
        None => run_tui(cli.file.as_deref()),
    }
}
