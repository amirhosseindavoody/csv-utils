use crate::column::{observe_column_infer, ColumnInferState};
use crate::column_stats::ColumnStatsAccum;
use crate::display::sanitize_ascii;

/// Incremental column width, type inference, and (lazy) statistics.
#[derive(Debug)]
pub struct ColumnLayoutState {
    pub rows_processed: usize,
    max_content_len: Vec<usize>,
    infer_state: Vec<ColumnInferState>,
    stats: Vec<ColumnStatsAccum>,
    stats_column: Option<usize>,
    stats_backfill_row: usize,
}

impl Default for ColumnLayoutState {
    fn default() -> Self {
        Self {
            rows_processed: 0,
            max_content_len: Vec::new(),
            infer_state: Vec::new(),
            stats: Vec::new(),
            stats_column: None,
            stats_backfill_row: 0,
        }
    }
}

impl ColumnLayoutState {
    pub fn reset_from_headers(&mut self, headers: &[String]) {
        let n = headers.len();
        self.rows_processed = 0;
        self.max_content_len = headers
            .iter()
            .map(|h| sanitize_ascii(h).len())
            .collect();
        self.infer_state = vec![ColumnInferState::Unknown; n];
        self.stats = vec![ColumnStatsAccum::default(); n];
        self.stats_column = None;
        self.stats_backfill_row = 0;
    }

    /// Width and inference for every column; stats only when the info panel is open.
    pub fn observe_fields(&mut self, fields: &[String]) {
        let n = self.max_content_len.len();
        for col in 0..n {
            if let Some(cell) = fields.get(col) {
                let len = sanitize_ascii(cell).len();
                self.max_content_len[col] = self.max_content_len[col].max(len);
                observe_column_infer(&mut self.infer_state[col], cell);
            }
        }
        self.rows_processed += 1;
    }

    pub fn set_stats_column(&mut self, col: Option<usize>) {
        self.stats_column = col;
        if let Some(c) = col {
            if c < self.stats.len() {
                self.stats[c] = ColumnStatsAccum::default();
            }
            self.stats_backfill_row = 0;
        }
    }

    pub fn stats_column(&self) -> Option<usize> {
        self.stats_column
    }

    pub fn stats_backfill_row(&self) -> usize {
        self.stats_backfill_row
    }

    pub fn advance_stats_backfill(&mut self) {
        self.stats_backfill_row += 1;
    }

    pub fn backfill_stats_for_row(&mut self, col: usize, fields: &[String]) {
        if let Some(cell) = fields.get(col) {
            self.stats[col].observe(cell);
        }
    }

    pub fn infer_state(&self, col: usize) -> ColumnInferState {
        self.infer_state
            .get(col)
            .copied()
            .unwrap_or(ColumnInferState::Unknown)
    }

    pub fn stats(&self, col: usize) -> ColumnStatsAccum {
        self.stats.get(col).cloned().unwrap_or_default()
    }

    pub fn max_content_len(&self, col: usize) -> usize {
        self.max_content_len.get(col).copied().unwrap_or(0)
    }

    pub fn column_count(&self) -> usize {
        self.max_content_len.len()
    }
}
