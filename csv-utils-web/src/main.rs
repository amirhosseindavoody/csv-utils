mod assets;
mod server;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "csv-utils-web", about = "Browser UI for csv-utils")]
struct Cli {
    /// CSV file to open
    file: Option<String>,

    /// Host address to bind
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// TCP port to listen on
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let file = cli.file.map(PathBuf::from);
    server::run(file, &cli.host, cli.port).await
}
