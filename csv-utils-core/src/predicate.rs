use crate::schema;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Eq,
    Neq,
    Gt,
    Lt,
    Contains,
    InList,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub column: String,
    pub op: Operator,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedCondition {
    pub index: usize,
    pub op: Operator,
    pub value: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PredicateError {
    #[error("invalid filter expression")]
    InvalidExpression,
    #[error("column not found: {0}")]
    ColumnNotFound(String),
}

pub fn parse_conditions(text: &str) -> Result<Vec<Condition>, PredicateError> {
    let mut out = Vec::new();
    for part in text.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }

        if let Some(sep) = p.find(" contains ") {
            let col = p[..sep].trim();
            let val = p[sep + " contains ".len()..].trim();
            if col.is_empty() {
                return Err(PredicateError::InvalidExpression);
            }
            out.push(Condition {
                column: col.to_string(),
                op: Operator::Contains,
                value: val.to_string(),
            });
            continue;
        }
        if let Some(sep) = p.find(" in ") {
            let col = p[..sep].trim();
            let val = p[sep + " in ".len()..].trim();
            if col.is_empty() {
                return Err(PredicateError::InvalidExpression);
            }
            out.push(Condition {
                column: col.to_string(),
                op: Operator::InList,
                value: val.to_string(),
            });
            continue;
        }
        if let Some(sep) = p.find("!=") {
            let col = p[..sep].trim();
            let val = p[sep + 2..].trim();
            if col.is_empty() {
                return Err(PredicateError::InvalidExpression);
            }
            out.push(Condition {
                column: col.to_string(),
                op: Operator::Neq,
                value: val.to_string(),
            });
            continue;
        }
        if let Some(sep) = p.find('>') {
            let col = p[..sep].trim();
            let val = p[sep + 1..].trim();
            if col.is_empty() {
                return Err(PredicateError::InvalidExpression);
            }
            out.push(Condition {
                column: col.to_string(),
                op: Operator::Gt,
                value: val.to_string(),
            });
            continue;
        }
        if let Some(sep) = p.find('<') {
            let col = p[..sep].trim();
            let val = p[sep + 1..].trim();
            if col.is_empty() {
                return Err(PredicateError::InvalidExpression);
            }
            out.push(Condition {
                column: col.to_string(),
                op: Operator::Lt,
                value: val.to_string(),
            });
            continue;
        }
        if let Some(sep) = p.find('=') {
            let col = p[..sep].trim();
            let val = p[sep + 1..].trim();
            if col.is_empty() {
                return Err(PredicateError::InvalidExpression);
            }
            out.push(Condition {
                column: col.to_string(),
                op: Operator::Eq,
                value: val.to_string(),
            });
            continue;
        }

        return Err(PredicateError::InvalidExpression);
    }
    Ok(out)
}

pub fn resolve_conditions(
    headers: &[String],
    conditions: &[Condition],
) -> Result<Vec<ResolvedCondition>, PredicateError> {
    let mut resolved = Vec::with_capacity(conditions.len());
    for cond in conditions {
        let index = schema::index_of(headers, &cond.column)
            .ok_or_else(|| PredicateError::ColumnNotFound(cond.column.clone()))?;
        resolved.push(ResolvedCondition {
            index,
            op: cond.op,
            value: cond.value.clone(),
        });
    }
    Ok(resolved)
}

pub fn row_matches_all(fields: &[String], conditions: &[ResolvedCondition]) -> bool {
    conditions.iter().all(|cond| {
        fields
            .get(cond.index)
            .is_some_and(|field| match_field(field, cond))
    })
}

fn match_field(field: &str, cond: &ResolvedCondition) -> bool {
    match cond.op {
        Operator::Eq => field == cond.value,
        Operator::Neq => field != cond.value,
        Operator::Contains => field.contains(&cond.value),
        Operator::Gt => compare_number(field, &cond.value, |a, b| a > b),
        Operator::Lt => compare_number(field, &cond.value, |a, b| a < b),
        Operator::InList => cond
            .value
            .split('|')
            .map(str::trim)
            .any(|item| field == item),
    }
}

fn compare_number(field: &str, rhs_text: &str, cmp: fn(f64, f64) -> bool) -> bool {
    let Ok(lhs) = field.parse::<f64>() else {
        return false;
    };
    let Ok(rhs) = rhs_text.parse::<f64>() else {
        return false;
    };
    cmp(lhs, rhs)
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operator::Eq => write!(f, "="),
            Operator::Neq => write!(f, "!="),
            Operator::Gt => write!(f, ">"),
            Operator::Lt => write!(f, "<"),
            Operator::Contains => write!(f, "contains"),
            Operator::InList => write!(f, "in"),
        }
    }
}
