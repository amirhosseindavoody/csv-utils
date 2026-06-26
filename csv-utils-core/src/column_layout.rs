use crate::column::{observe_column_infer, ColumnInferState};
use crate::column_stats::ColumnStatsAccum;
use crate::display::sanitize_ascii;

/// Incremental column width, type inference, and statistics.
#[derive(Debug)]
pub struct ColumnLayoutState {
    pub rows_processed: usize,
    max_content_len: Vec<usize>,
    infer_state: Vec<ColumnInferState>,
    stats: Vec<ColumnStatsAccum>,
}

impl Default for ColumnLayoutState {
    fn default() -> Self {
        Self {
            rows_processed: 0,
            max_content_len: Vec::new(),
            infer_state: Vec::new(),
            stats: Vec::new(),
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
    }

    /// Width, inference, and statistics for every column as rows are indexed.
    pub fn observe_fields(&mut self, fields: &[String]) {
        let n = self.max_content_len.len();
        for col in 0..n {
            if let Some(cell) = fields.get(col) {
                let len = sanitize_ascii(cell).len();
                self.max_content_len[col] = self.max_content_len[col].max(len);
                observe_column_infer(&mut self.infer_state[col], cell);
                self.stats[col].observe(cell);
            }
        }
        self.rows_processed += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_fields_accumulates_stats_for_all_columns() {
        let mut layout = ColumnLayoutState::default();
        layout.reset_from_headers(&["a".to_string(), "b".to_string()]);
        layout.observe_fields(&["1".to_string(), "2".to_string()]);
        layout.observe_fields(&["3".to_string(), "".to_string()]);

        let stats_a = layout.stats(0);
        assert_eq!(stats_a.rows, 2);
        assert_eq!(stats_a.nulls, 0);
        assert_eq!(stats_a.int_min, Some(1));
        assert_eq!(stats_a.int_max, Some(3));

        let stats_b = layout.stats(1);
        assert_eq!(stats_b.rows, 2);
        assert_eq!(stats_b.nulls, 1);
    }
}
