use anyhow::Result;
use csv_utils_core::engine::{self, EngineError};
use std::io::{self, Write};
use std::path::Path;

pub fn run_stats(file: &str) -> Result<()> {
    run_engine(file, |path, out| engine::print_basic_stats(path, out))
}

pub fn run_unique(file: &str, columns: &str, limit: usize) -> Result<()> {
    run_engine(file, |path, out| engine::print_unique_values(path, columns, limit, out))
}

pub fn run_json(file: &str, limit: usize) -> Result<()> {
    run_engine(file, |path, out| engine::print_rows_as_json(path, limit, out))
}

pub fn run_filter(file: &str, expr: &str, limit: usize) -> Result<()> {
    run_engine(file, |path, out| engine::print_filtered_rows(path, expr, limit, out))
}

fn run_engine(
    file: &str,
    f: impl FnOnce(&Path, &mut dyn Write) -> Result<(), EngineError>,
) -> Result<()> {
    let path = Path::new(file);
    f(path, &mut io::stdout()).map_err(map_engine_error)
}

fn map_engine_error(err: EngineError) -> anyhow::Error {
    match err {
        EngineError::Io(e) => e.into(),
        other => anyhow::Error::msg(other.to_string()),
    }
}
