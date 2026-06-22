use crate::schema;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

const READ_BUF_SIZE: usize = 1024 * 1024;

pub fn print_unique_for_columns(
    path: &std::path::Path,
    headers: &[String],
    indexes: &[usize],
    cap: usize,
    mut out: impl Write,
) -> std::io::Result<()> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(READ_BUF_SIZE, file);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    line.clear();

    let mut seen = HashSet::new();
    while seen.len() < cap {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let fields = schema::split_row(line.trim_end());
        let key = build_key(&fields, indexes);
        if !seen.insert(key.clone()) {
            continue;
        }
        write_json_combo(&mut out, headers, indexes, &key)?;
    }
    Ok(())
}

fn build_key(fields: &[String], indexes: &[usize]) -> String {
    let mut parts = Vec::with_capacity(indexes.len());
    for &idx in indexes {
        if let Some(val) = fields.get(idx) {
            parts.push(val.as_str());
        }
    }
    parts.join("\x1f")
}

fn write_json_combo(
    out: &mut impl Write,
    headers: &[String],
    indexes: &[usize],
    key: &str,
) -> std::io::Result<()> {
    let values: Vec<&str> = key.split('\x1f').collect();
    write!(out, "{{")?;
    for (i, &idx) in indexes.iter().enumerate() {
        if i != 0 {
            write!(out, ", ")?;
        }
        let name = headers.get(idx).map(String::as_str).unwrap_or("unknown");
        let val = values.get(i).copied().unwrap_or("");
        write!(out, "\"{name}\": \"{val}\"")?;
    }
    writeln!(out, "}}")?;
    Ok(())
}
