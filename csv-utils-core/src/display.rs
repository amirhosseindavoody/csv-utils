use crate::column::{ColumnKind, NumericRepr};

const ELLIPSIS: &str = "...";

/// Sanitize to printable ASCII (non-printable → `.`).
pub fn sanitize_ascii(text: &str) -> String {
    text.bytes()
        .map(|b| if (32..=126).contains(&b) { b as char } else { '.' })
        .collect()
}

pub fn truncate_middle(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let visible = sanitize_ascii(text);
    if visible.len() <= width {
        return visible;
    }
    if width <= ELLIPSIS.len() {
        return ".".repeat(width);
    }
    let keep = width - ELLIPSIS.len();
    let left = keep / 2;
    let right = keep - left;
    let left_part = &visible[..left];
    let right_part = &visible[visible.len() - right..];
    format!("{left_part}{ELLIPSIS}{right_part}")
}

fn pad_left(text: &str, width: usize) -> String {
    if text.len() >= width {
        return text.to_string();
    }
    format!("{:>width$}", text, width = width)
}

fn pad_right(text: &str, width: usize) -> String {
    if text.len() >= width {
        return text.to_string();
    }
    format!("{:<width$}", text, width = width)
}

fn fits(s: &str, width: usize) -> bool {
    s.len() <= width
}

fn format_float_general(n: f64, prec: usize) -> String {
    let s = format!("{n:.prec$}");
    if s.contains('e') || s.contains('E') {
        s
    } else {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn format_float_scientific(n: f64, prec: usize) -> String {
    format!("{n:.prec$e}")
}

fn format_int_plain(n: i64) -> String {
    n.to_string()
}

fn format_int_scientific(n: i64, prec: usize) -> String {
    format!("{n:.prec$e}")
}

pub fn format_numeric_cell(text: &str, width: usize, kind: ColumnKind, repr: NumericRepr) -> String {
    if width == 0 {
        return String::new();
    }
    let sanitized = sanitize_ascii(text);
    if fits(&sanitized, width) {
        return pad_left(&sanitized, width);
    }

    match kind {
        ColumnKind::Int => {
            if let Ok(n) = sanitized.parse::<i64>() {
                let plain = format_int_plain(n);
                if fits(&plain, width) {
                    return pad_left(&plain, width);
                }
                for prec in (0..=6).rev() {
                    let s = format_int_scientific(n, prec);
                    if fits(&s, width) {
                        return pad_left(&s, width);
                    }
                }
                for prec in (0..=3).rev() {
                    let s = format!("{n:.prec$e}");
                    if fits(&s, width) {
                        return pad_left(&s, width);
                    }
                }
            }
        }
        ColumnKind::Float => {
            if let Ok(n) = sanitized.parse::<f64>() {
                let try_order: &[NumericRepr] = match repr {
                    NumericRepr::General => &[NumericRepr::General, NumericRepr::Scientific],
                    NumericRepr::Scientific => &[NumericRepr::Scientific, NumericRepr::General],
                };
                for &r in try_order {
                    for prec in (0..=12).rev() {
                        let s = match r {
                            NumericRepr::General => format_float_general(n, prec),
                            NumericRepr::Scientific => format_float_scientific(n, prec),
                        };
                        if fits(&s, width) {
                            return pad_left(&s, width);
                        }
                    }
                }
            }
        }
        _ => {}
    }

    pad_left(&truncate_middle(&sanitized, width), width)
}

pub fn format_cell_for_column(
    text: &str,
    width: usize,
    kind: ColumnKind,
    repr: NumericRepr,
) -> String {
    if width == 0 {
        return String::new();
    }
    match kind {
        ColumnKind::Text | ColumnKind::Date => pad_right(&truncate_middle(text, width), width),
        ColumnKind::Int | ColumnKind::Float => format_numeric_cell(text, width, kind, repr),
        ColumnKind::Auto => pad_right(&truncate_middle(text, width), width),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middle_truncation() {
        assert_eq!(truncate_middle("hello world", 11), "hello world");
        assert_eq!(truncate_middle("hello world", 5), "h...d");
    }

    #[test]
    fn numeric_rescales_long_float() {
        let s = format_numeric_cell(
            "770.111483577523",
            10,
            ColumnKind::Float,
            NumericRepr::General,
        );
        assert_eq!(s.len(), 10);
        assert!(!s.contains('~'));
        assert!(!s.contains("..."));
    }

    #[test]
    fn numeric_rescales_scientific() {
        let s = format_numeric_cell(
            "8.302113087438814e-11",
            8,
            ColumnKind::Float,
            NumericRepr::Scientific,
        );
        assert_eq!(s.len(), 8);
    }

    #[test]
    fn text_uses_middle_ellipsis() {
        let s = format_cell_for_column("hello world", 8, ColumnKind::Text, NumericRepr::General);
        assert_eq!(s, "he...rld");
    }
}
