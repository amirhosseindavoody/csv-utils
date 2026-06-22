#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Str,
    LongStr,
    FloatGeneral,
    FloatScientific,
    FloatMixed,
    Int,
    Date,
    Unknown,
}

impl ColumnKind {
    pub fn label(self) -> &'static str {
        match self {
            ColumnKind::Str => "str",
            ColumnKind::LongStr => "long_str",
            ColumnKind::FloatGeneral => "float",
            ColumnKind::FloatScientific => "float_sci",
            ColumnKind::FloatMixed => "float_mix",
            ColumnKind::Int => "int",
            ColumnKind::Date => "date",
            ColumnKind::Unknown => "?",
        }
    }
}

pub fn infer_column_kind(name: &str) -> ColumnKind {
    if name.starts_with("long_str_") {
        ColumnKind::LongStr
    } else if name.starts_with("float_general_") {
        ColumnKind::FloatGeneral
    } else if name.starts_with("float_scientific_") {
        ColumnKind::FloatScientific
    } else if name.starts_with("float_mixed_") {
        ColumnKind::FloatMixed
    } else if name.starts_with("int_") {
        ColumnKind::Int
    } else if name.starts_with("date_") {
        ColumnKind::Date
    } else if name.starts_with("str_") {
        ColumnKind::Str
    } else {
        ColumnKind::Unknown
    }
}

pub fn is_right_aligned(kind: ColumnKind) -> bool {
    matches!(
        kind,
        ColumnKind::Int | ColumnKind::FloatGeneral | ColumnKind::FloatScientific | ColumnKind::FloatMixed
    )
}
