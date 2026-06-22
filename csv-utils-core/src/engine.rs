use crate::json_view;
use crate::predicate::{self, PredicateError};
use crate::schema;
use crate::stats::StatsAgg;
use crate::unique;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

const READ_BUF_SIZE: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("empty csv")]
    EmptyCsv,
    #[error("missing column name")]
    MissingColumnName,
    #[error("missing filter expression")]
    MissingFilterExpression,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Predicate(#[from] PredicateError),
}

pub fn print_basic_stats(path: &Path, mut out: impl Write) -> Result<(), EngineError> {
    let (headers, rows) = read_all_rows(path)?;
    let mut agg = StatsAgg::new(headers.len());
    for fields in rows {
        agg.observe(&fields);
    }
    write!(out, "{}", agg.print(&headers))?;
    Ok(())
}

pub fn print_unique_values(
    path: &Path,
    columns_expr: &str,
    cap: usize,
    mut out: impl Write,
) -> Result<(), EngineError> {
    let headers = read_headers(path)?;
    let indexes = resolve_column_indexes(&headers, columns_expr)?;
    unique::print_unique_for_columns(path, &headers, &indexes, cap, &mut out)?;
    Ok(())
}

pub fn print_rows_as_json(path: &Path, limit: usize, mut out: impl Write) -> Result<(), EngineError> {
    let (headers, rows) = read_rows_limited(path, limit)?;
    for fields in rows {
        json_view::print_row(&headers, &fields, &mut out)?;
        writeln!(out)?;
    }
    Ok(())
}

pub fn print_filtered_rows(
    path: &Path,
    filter_expr: &str,
    limit: usize,
    mut out: impl Write,
) -> Result<(), EngineError> {
    let headers = read_headers(path)?;
    let conditions = predicate::parse_conditions(filter_expr)?;
    if conditions.is_empty() {
        return Err(EngineError::Predicate(PredicateError::InvalidExpression));
    }
    let resolved = predicate::resolve_conditions(&headers, &conditions)?;

    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(READ_BUF_SIZE, file);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    line.clear();

    let mut emitted = 0usize;
    while emitted < limit {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let fields = schema::split_row(line.trim_end());
        if predicate::row_matches_all(&fields, &resolved) {
            json_view::print_row(&headers, &fields, &mut out)?;
            writeln!(out)?;
            emitted += 1;
        }
    }
    Ok(())
}

fn read_headers(path: &Path) -> Result<Vec<String>, EngineError> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(READ_BUF_SIZE, file);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(EngineError::EmptyCsv);
    }
    Ok(schema::split_row(line.trim_end()))
}

fn read_all_rows(path: &Path) -> Result<(Vec<String>, Vec<Vec<String>>), EngineError> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(READ_BUF_SIZE, file);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(EngineError::EmptyCsv);
    }
    let headers = schema::split_row(line.trim_end());
    let mut rows = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        rows.push(schema::split_row(line.trim_end()));
    }
    Ok((headers, rows))
}

fn read_rows_limited(path: &Path, limit: usize) -> Result<(Vec<String>, Vec<Vec<String>>), EngineError> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(READ_BUF_SIZE, file);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(EngineError::EmptyCsv);
    }
    let headers = schema::split_row(line.trim_end());
    let mut rows = Vec::new();
    while rows.len() < limit {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        rows.push(schema::split_row(line.trim_end()));
    }
    Ok((headers, rows))
}

fn resolve_column_indexes(headers: &[String], columns_expr: &str) -> Result<Vec<usize>, EngineError> {
    let mut indexes = Vec::new();
    for part in columns_expr.split(',') {
        let col = part.trim();
        if col.is_empty() {
            continue;
        }
        let idx = schema::index_of(headers, col)
            .ok_or_else(|| EngineError::Predicate(PredicateError::ColumnNotFound(col.to_string())))?;
        indexes.push(idx);
    }
    if indexes.is_empty() {
        return Err(EngineError::MissingColumnName);
    }
    Ok(indexes)
}

pub fn print_help(mut out: impl Write) -> io::Result<()> {
    writeln!(
        out,
        "csv-utils: high-performance CSV CLI + TUI\n\n\
         Usage:\n\
           csv-utils stats <file.csv>\n\
           csv-utils unique <file.csv> <col1[,col2,...]> [limit]\n\
           csv-utils json <file.csv> [limit]\n\
           csv-utils filter <file.csv> <expr> [limit]\n\
             operators: =, !=, >, <, contains, in\n\
             examples:\n\
               city=Tehran,active=true\n\
               age>30\n\
               name contains Ali\n\
               city in Tehran|Paris\n\
           csv-utils tui [file.csv]\n"
    )
}
