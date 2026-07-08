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

/// Sort `indices` using precomputed `cells` (one cell string per index).
///
/// Extracts a [`SortKey`] once per row, then sorts keys in memory. Callers should
/// build `cells` with a single pass over the file (one `row_fields` per row)
/// instead of parsing inside each comparison.
pub fn sort_indices_by_cells(
    kind: ColumnKind,
    direction: SortDirection,
    indices: &mut [usize],
    cells: &[String],
) {
    debug_assert_eq!(indices.len(), cells.len());
    let mut keyed: Vec<(SortKey, usize)> = cells
        .iter()
        .zip(indices.iter().copied())
        .map(|(cell, row)| (sort_key_for_cell(kind, cell), row))
        .collect();
    keyed.sort_by(|(key_a, row_a), (key_b, row_b)| {
        let ord = compare_sort_keys(key_a, key_b);
        let ord = match direction {
            SortDirection::Ascending => ord,
            SortDirection::Descending => ord.reverse(),
        };
        ord.then_with(|| row_a.cmp(row_b))
    });
    for (i, (_, row)) in keyed.into_iter().enumerate() {
        indices[i] = row;
    }
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

    #[test]
    fn sort_indices_by_cells_orders_once_per_row() {
        let mut indices = vec![0, 1, 2, 3];
        let cells = vec![
            "10".to_string(),
            "2".to_string(),
            "".to_string(),
            "3".to_string(),
        ];
        sort_indices_by_cells(
            ColumnKind::Int,
            SortDirection::Ascending,
            &mut indices,
            &cells,
        );
        assert_eq!(indices, vec![1, 3, 0, 2]);
    }
}
