#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Date,
    Int,
    Float,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericRepr {
    General,
    Scientific,
}

impl ColumnKind {
    pub fn label(self) -> &'static str {
        match self {
            ColumnKind::Text => "text",
            ColumnKind::Date => "date",
            ColumnKind::Int => "int",
            ColumnKind::Float => "float",
            ColumnKind::Auto => "auto",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            ColumnKind::Auto => ColumnKind::Text,
            ColumnKind::Text => ColumnKind::Date,
            ColumnKind::Date => ColumnKind::Int,
            ColumnKind::Int => ColumnKind::Float,
            ColumnKind::Float => ColumnKind::Auto,
        }
    }
}

pub fn column_kind_from_label(label: &str) -> Option<ColumnKind> {
    match label {
        "auto" => Some(ColumnKind::Auto),
        "text" => Some(ColumnKind::Text),
        "date" => Some(ColumnKind::Date),
        "int" => Some(ColumnKind::Int),
        "float" => Some(ColumnKind::Float),
        _ => None,
    }
}

pub fn numeric_repr_from_label(label: &str) -> Option<NumericRepr> {
    match label {
        "general" => Some(NumericRepr::General),
        "scientific" => Some(NumericRepr::Scientific),
        _ => None,
    }
}

impl NumericRepr {
    pub fn label(self) -> &'static str {
        match self {
            NumericRepr::General => "general",
            NumericRepr::Scientific => "scientific",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            NumericRepr::General => NumericRepr::Scientific,
            NumericRepr::Scientific => NumericRepr::General,
        }
    }
}

pub fn column_kind_options() -> &'static [ColumnKind] {
    &[
        ColumnKind::Auto,
        ColumnKind::Text,
        ColumnKind::Date,
        ColumnKind::Int,
        ColumnKind::Float,
    ]
}

pub fn column_kind_index(kind: ColumnKind) -> usize {
    column_kind_options()
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0)
}

pub fn numeric_repr_options() -> &'static [NumericRepr] {
    &[NumericRepr::General, NumericRepr::Scientific]
}

pub fn numeric_repr_index(repr: NumericRepr) -> usize {
    match repr {
        NumericRepr::General => 0,
        NumericRepr::Scientific => 1,
    }
}

pub fn is_numeric(kind: ColumnKind) -> bool {
    matches!(kind, ColumnKind::Int | ColumnKind::Float)
}

pub fn uses_middle_ellipsis(kind: ColumnKind) -> bool {
    matches!(kind, ColumnKind::Text | ColumnKind::Date)
}

pub fn is_right_aligned(kind: ColumnKind) -> bool {
    is_numeric(kind)
}

fn is_date_value(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(|c| c.is_ascii_digit())
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

fn is_int_value(s: &str) -> bool {
    s.parse::<i64>().is_ok() && !s.contains('.') && !s.contains('e') && !s.contains('E')
}

fn is_float_value(s: &str) -> bool {
    s.parse::<f64>().is_ok()
}

pub fn infer_column_kind_from_values(values: &[&str]) -> ColumnKind {
    let mut state = ColumnInferState::Unknown;
    for value in values {
        observe_column_infer(&mut state, value);
    }
    infer_kind_from_state(state)
}

/// Incremental type inference state (updated one cell at a time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnInferState {
    #[default]
    Unknown,
    Date,
    Int,
    Float,
    Text,
}

pub fn observe_column_infer(state: &mut ColumnInferState, value: &str) {
    if value.is_empty() {
        return;
    }
    match *state {
        ColumnInferState::Unknown => {
            if is_date_value(value) {
                *state = ColumnInferState::Date;
            } else if is_int_value(value) {
                *state = ColumnInferState::Int;
            } else if is_float_value(value) {
                *state = ColumnInferState::Float;
            } else {
                *state = ColumnInferState::Text;
            }
        }
        ColumnInferState::Date => {
            if !is_date_value(value) {
                *state = ColumnInferState::Text;
            }
        }
        ColumnInferState::Int => {
            if is_int_value(value) {
            } else if is_float_value(value) {
                *state = ColumnInferState::Float;
            } else {
                *state = ColumnInferState::Text;
            }
        }
        ColumnInferState::Float => {
            if !is_float_value(value) {
                *state = ColumnInferState::Text;
            }
        }
        ColumnInferState::Text => {}
    }
}

pub fn infer_kind_from_state(state: ColumnInferState) -> ColumnKind {
    match state {
        ColumnInferState::Unknown => ColumnKind::Text,
        ColumnInferState::Date => ColumnKind::Date,
        ColumnInferState::Int => ColumnKind::Int,
        ColumnInferState::Float => ColumnKind::Float,
        ColumnInferState::Text => ColumnKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_date_column() {
        let vals = ["2018-12-23", "2020-01-01", ""];
        assert_eq!(infer_column_kind_from_values(&vals), ColumnKind::Date);
    }

    #[test]
    fn infers_int_column() {
        let vals = ["123", "-456", "0"];
        assert_eq!(infer_column_kind_from_values(&vals), ColumnKind::Int);
    }

    #[test]
    fn infers_float_column() {
        let vals = ["1.5", "-2.3e-4", "100"];
        assert_eq!(infer_column_kind_from_values(&vals), ColumnKind::Float);
    }

    #[test]
    fn infers_float_column_int_first() {
        let vals = ["100", "1.5", "-2.3e-4"];
        assert_eq!(infer_column_kind_from_values(&vals), ColumnKind::Float);
    }

    #[test]
    fn infers_text_column() {
        let vals = ["hello", "123", "world"];
        assert_eq!(infer_column_kind_from_values(&vals), ColumnKind::Text);
    }

    #[test]
    fn empty_values_default_to_text() {
        assert_eq!(infer_column_kind_from_values(&[]), ColumnKind::Text);
        assert_eq!(infer_column_kind_from_values(&[""]), ColumnKind::Text);
    }

    #[test]
    fn incremental_inference_matches_batch() {
        let vals = ["2018-12-23", "2020-01-01", "", "2019-06-01"];
        let mut state = ColumnInferState::Unknown;
        for v in vals {
            observe_column_infer(&mut state, v);
        }
        assert_eq!(infer_kind_from_state(state), infer_column_kind_from_values(&vals));
    }
}
