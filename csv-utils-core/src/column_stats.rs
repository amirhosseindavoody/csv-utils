use crate::column::{is_date_value, is_float_value, is_int_value, is_numeric, ColumnKind, NumericRepr};
use serde::Serialize;
use std::collections::HashSet;

const DISTINCT_CAP: usize = 10_000;

#[derive(Debug, Clone, Default)]
pub struct ColumnStatsAccum {
    pub rows: usize,
    pub nulls: usize,
    pub min_len: Option<usize>,
    pub max_len: usize,
    distinct: HashSet<String>,
    distinct_capped: bool,
    pub date_min: Option<String>,
    pub date_max: Option<String>,
    pub int_min: Option<i64>,
    pub int_max: Option<i64>,
    pub int_sum: i128,
    pub int_count: usize,
    pub float_min: Option<f64>,
    pub float_max: Option<f64>,
    pub float_sum: f64,
    pub float_count: usize,
}

impl ColumnStatsAccum {
    pub fn observe(&mut self, value: &str) {
        self.rows += 1;
        if value.is_empty() {
            self.nulls += 1;
            return;
        }

        let len = value.len();
        self.min_len = Some(self.min_len.map_or(len, |m| m.min(len)));
        self.max_len = self.max_len.max(len);

        if !self.distinct_capped {
            if self.distinct.len() >= DISTINCT_CAP && !self.distinct.contains(value) {
                self.distinct_capped = true;
            } else {
                self.distinct.insert(value.to_string());
            }
        }

        if is_date_value(value) {
            update_min_max_string(&mut self.date_min, &mut self.date_max, value);
        }
        if is_int_value(value) {
            if let Ok(v) = value.parse::<i64>() {
                self.int_count += 1;
                self.int_sum += v as i128;
                self.int_min = Some(self.int_min.map_or(v, |m| m.min(v)));
                self.int_max = Some(self.int_max.map_or(v, |m| m.max(v)));
            }
        }
        if is_float_value(value) {
            if let Ok(v) = value.parse::<f64>() {
                if v.is_finite() {
                    self.float_count += 1;
                    self.float_sum += v;
                    self.float_min = Some(self.float_min.map_or(v, |m| m.min(v)));
                    self.float_max = Some(self.float_max.map_or(v, |m| m.max(v)));
                }
            }
        }
    }

    pub fn distinct_count_label(&self) -> String {
        if self.distinct_capped {
            format!("≥{DISTINCT_CAP} (in loaded rows)")
        } else {
            self.distinct.len().to_string()
        }
    }
}

fn update_min_max_string(min: &mut Option<String>, max: &mut Option<String>, value: &str) {
    if min.is_none() {
        *min = Some(value.to_string());
        *max = Some(value.to_string());
        return;
    }
    if value < min.as_ref().unwrap().as_str() {
        *min = Some(value.to_string());
    }
    if max.as_ref().is_none_or(|m| value > m.as_str()) {
        *max = Some(value.to_string());
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfoStat {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub column_index: usize,
    pub column_name: String,
    pub stored_kind: String,
    pub effective_kind: String,
    pub numeric_repr: Option<String>,
    pub rows_sampled: usize,
    pub scan_complete: bool,
    pub stats: Vec<ColumnInfoStat>,
    pub available_types: Vec<String>,
    pub focus: usize,
    pub repr_section_visible: bool,
    pub repr_enabled: bool,
    pub decimal_section_visible: bool,
    pub decimal_format: Option<String>,
    pub decimal_editing: bool,
    pub decimal_draft: String,
}

pub fn build_column_info(
    col: usize,
    column_name: &str,
    stored: ColumnKind,
    effective: ColumnKind,
    repr: NumericRepr,
    stats: &ColumnStatsAccum,
    scan_complete: bool,
    focus: usize,
    repr_section_visible: bool,
    repr_enabled: bool,
    available_types: &[ColumnKind],
    decimal_section_visible: bool,
    decimal_format: Option<String>,
    decimal_editing: bool,
    decimal_draft: String,
) -> ColumnInfo {
    let mut lines = vec![
        stat("Rows sampled", stats.rows.to_string()),
        stat("Empty", stats.nulls.to_string()),
        stat("Non-empty", (stats.rows.saturating_sub(stats.nulls)).to_string()),
    ];

    if !scan_complete {
        lines.push(stat("Note", "Statistics from loaded rows only".to_string()));
    }

    match effective {
        ColumnKind::Text => {
            lines.push(stat("Distinct values", stats.distinct_count_label()));
            if let Some(min) = stats.min_len {
                lines.push(stat("Min length", min.to_string()));
            }
            lines.push(stat("Max length", stats.max_len.to_string()));
        }
        ColumnKind::Date => {
            if let Some(min) = &stats.date_min {
                lines.push(stat("Earliest", min.clone()));
            }
            if let Some(max) = &stats.date_max {
                lines.push(stat("Latest", max.clone()));
            }
        }
        ColumnKind::Int => {
            if let Some(min) = stats.int_min {
                lines.push(stat("Min", min.to_string()));
            }
            if let Some(max) = stats.int_max {
                lines.push(stat("Max", max.to_string()));
            }
            if stats.int_count > 0 {
                let mean = stats.int_sum as f64 / stats.int_count as f64;
                lines.push(stat("Mean", format_float(mean)));
            }
        }
        ColumnKind::Float => {
            if let Some(min) = stats.float_min {
                lines.push(stat("Min", format_float(min)));
            }
            if let Some(max) = stats.float_max {
                lines.push(stat("Max", format_float(max)));
            }
            if stats.float_count > 0 {
                let mean = stats.float_sum / stats.float_count as f64;
                lines.push(stat("Mean", format_float(mean)));
            }
        }
        ColumnKind::Auto => {
            lines.push(stat("Distinct values", stats.distinct_count_label()));
            if let Some(min) = stats.min_len {
                lines.push(stat("Min length", min.to_string()));
            }
            lines.push(stat("Max length", stats.max_len.to_string()));
        }
    }

    let numeric_repr = if is_numeric(effective) {
        Some(repr.label().to_string())
    } else {
        None
    };

    ColumnInfo {
        column_index: col,
        column_name: column_name.to_string(),
        stored_kind: stored.label().to_string(),
        effective_kind: effective.label().to_string(),
        numeric_repr,
        rows_sampled: stats.rows,
        scan_complete,
        stats: lines,
        available_types: available_types
            .iter()
            .map(|k| k.label().to_string())
            .collect(),
        focus,
        repr_section_visible,
        repr_enabled,
        decimal_section_visible,
        decimal_format,
        decimal_editing,
        decimal_draft,
    }
}

fn stat(label: &str, value: String) -> ColumnInfoStat {
    ColumnInfoStat {
        label: label.to_string(),
        value,
    }
}

fn format_float(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{:.6}", trim_float(v))
    }
}

fn trim_float(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_int_stats() {
        let mut s = ColumnStatsAccum::default();
        for v in ["1", "3", "5", ""] {
            s.observe(v);
        }
        assert_eq!(s.rows, 4);
        assert_eq!(s.nulls, 1);
        assert_eq!(s.int_min, Some(1));
        assert_eq!(s.int_max, Some(5));
        assert_eq!(s.int_count, 3);
    }

    #[test]
    fn info_for_text_column() {
        let mut s = ColumnStatsAccum::default();
        s.observe("hello");
        s.observe("world");
        let info = build_column_info(
            0,
            "name",
            ColumnKind::Text,
            ColumnKind::Text,
            NumericRepr::General,
            &s,
            true,
            0,
            false,
            false,
            &[ColumnKind::Text],
            false,
            None,
            false,
            String::new(),
        );
        assert!(info.stats.iter().any(|l| l.label == "Distinct values"));
    }
}
