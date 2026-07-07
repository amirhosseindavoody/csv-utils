//! Diagnostic microbenchmark: how long does it take to populate and then drop
//! a `ColumnLayoutState` sized like a fully-scanned wide/tall CSV?
//!
//! Run with: cargo run --release --example bench_stats_drop -p csv-utils-core

use csv_utils_core::column_layout::ColumnLayoutState;
use std::time::Instant;

fn main() {
    let cols = 1000usize;
    let rows = 10_000usize;

    let headers: Vec<String> = (0..cols).map(|c| format!("col_{c}")).collect();
    let mut layout = ColumnLayoutState::default();
    layout.reset_from_headers(&headers);

    let populate_start = Instant::now();
    for r in 0..rows {
        let fields: Vec<String> = (0..cols).map(|c| format!("val_{r}_{c}")).collect();
        layout.observe_fields(&fields);
    }
    let populate_elapsed = populate_start.elapsed();
    println!("populate {rows} rows x {cols} cols: {populate_elapsed:?}");

    let drop_start = Instant::now();
    drop(layout);
    let drop_elapsed = drop_start.elapsed();
    println!("drop populated ColumnLayoutState: {drop_elapsed:?}");
}
