use crate::column::{is_date_value, is_float_value, is_int_value, ColumnKind};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn toggle(self) -> Self {
        match self {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SortKey {
    Empty,
    Text(String),
    Int(i64),
    Float(f64),
    Date(String),
}

fn sort_key_for_cell(kind: ColumnKind, cell: &str) -> SortKey {
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        return SortKey::Empty;
    }
    match kind {
        ColumnKind::Int => {
            if let Ok(value) = trimmed.parse::<i64>() {
                SortKey::Int(value)
            } else {
                SortKey::Text(trimmed.to_string())
            }
        }
        ColumnKind::Float => {
            if let Ok(value) = trimmed.parse::<f64>() {
                SortKey::Float(value)
            } else {
                SortKey::Text(trimmed.to_string())
            }
        }
        ColumnKind::Date => {
            if is_date_value(trimmed) {
                SortKey::Date(trimmed.to_string())
            } else {
                SortKey::Text(trimmed.to_string())
            }
        }
        ColumnKind::Text | ColumnKind::Auto => {
            if is_date_value(trimmed) {
                SortKey::Date(trimmed.to_string())
            } else if is_int_value(trimmed) {
                SortKey::Int(trimmed.parse().unwrap_or(0))
            } else if is_float_value(trimmed) {
                SortKey::Float(trimmed.parse().unwrap_or(0.0))
            } else {
                SortKey::Text(trimmed.to_string())
            }
        }
    }
}

fn compare_sort_keys(a: &SortKey, b: &SortKey) -> Ordering {
    use SortKey::*;
    match (a, b) {
        (Empty, Empty) => Ordering::Equal,
        (Empty, _) => Ordering::Greater,
        (_, Empty) => Ordering::Less,
        (Int(x), Int(y)) => x.cmp(y),
        (Float(x), Float(y)) => x.total_cmp(y),
        (Date(x), Date(y)) => x.cmp(y),
        (Text(x), Text(y)) => x.cmp(y),
        _ => sortable_text(a).cmp(&sortable_text(b)),
    }
}

fn sortable_text(key: &SortKey) -> String {
    match key {
        SortKey::Empty => String::new(),
        SortKey::Text(s) | SortKey::Date(s) => s.clone(),
        SortKey::Int(n) => n.to_string(),
        SortKey::Float(f) => f.to_string(),
    }
}

pub fn compare_cells(kind: ColumnKind, a: &str, b: &str) -> Ordering {
    compare_sort_keys(&sort_key_for_cell(kind, a), &sort_key_for_cell(kind, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_integers_numerically() {
        assert_eq!(
            compare_cells(ColumnKind::Int, "10", "2"),
            Ordering::Greater
        );
    }

    #[test]
    fn compares_floats_numerically() {
        assert_eq!(
            compare_cells(ColumnKind::Float, "1.5", "1.25"),
            Ordering::Greater
        );
    }

    #[test]
    fn compares_dates_lexicographically() {
        assert_eq!(
            compare_cells(ColumnKind::Date, "2024-02-01", "2024-01-31"),
            Ordering::Greater
        );
    }

    #[test]
    fn empty_cells_sort_last() {
        assert_eq!(
            compare_cells(ColumnKind::Text, "", "a"),
            Ordering::Greater
        );
        assert_eq!(
            compare_cells(ColumnKind::Text, "a", ""),
            Ordering::Less
        );
    }

    #[test]
    fn auto_kind_infers_numeric_order() {
        assert_eq!(
            compare_cells(ColumnKind::Auto, "10", "2"),
            Ordering::Greater
        );
    }
}
