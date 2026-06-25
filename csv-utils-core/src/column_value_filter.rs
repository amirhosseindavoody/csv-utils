use crate::fuzzy::fuzzy_score;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ColumnFilterError {
    #[error("invalid filter expression")]
    InvalidExpression,
    #[error("expected numeric value in cell")]
    NonNumericCell,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CmpOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Compare(CmpOp, f64),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse_expr(&mut self) -> Result<Expr, ColumnFilterError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ColumnFilterError> {
        let mut left = self.parse_and()?;
        while self.consume('|') {
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ColumnFilterError> {
        let mut left = self.parse_primary()?;
        while self.consume('&') {
            let right = self.parse_primary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr, ColumnFilterError> {
        self.skip_ws();
        if self.consume('(') {
            let expr = self.parse_or()?;
            self.skip_ws();
            if !self.consume(')') {
                return Err(ColumnFilterError::InvalidExpression);
            }
            Ok(expr)
        } else {
            self.parse_compare()
        }
    }

    fn parse_compare(&mut self) -> Result<Expr, ColumnFilterError> {
        self.skip_ws();
        let op = if self.try_consume(">=") {
            CmpOp::Ge
        } else if self.try_consume("<=") {
            CmpOp::Le
        } else if self.try_consume("==") {
            CmpOp::Eq
        } else if self.try_consume("!=") {
            CmpOp::Ne
        } else if self.consume('>') {
            CmpOp::Gt
        } else if self.consume('<') {
            CmpOp::Lt
        } else {
            return Err(ColumnFilterError::InvalidExpression);
        };
        self.skip_ws();
        let value = self.parse_number()?;
        Ok(Expr::Compare(op, value))
    }

    fn parse_number(&mut self) -> Result<f64, ColumnFilterError> {
        self.skip_ws();
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
        {
            self.pos += 1;
        }
        if self.pos == start || (self.pos == start + 1 && self.input.as_bytes()[start] == b'-') {
            return Err(ColumnFilterError::InvalidExpression);
        }
        self.input[start..self.pos]
            .parse::<f64>()
            .map_err(|_| ColumnFilterError::InvalidExpression)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn consume(&mut self, ch: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(ch) {
            self.pos += ch.len_utf8();
            true
        } else {
            false
        }
    }

    fn try_consume(&mut self, s: &str) -> bool {
        self.skip_ws();
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn finished(&mut self) {
        self.skip_ws();
    }
}

fn parse_numeric_filter(expr: &str) -> Result<Expr, ColumnFilterError> {
    let mut parser = Parser::new(expr.trim());
    let expr = parser.parse_expr()?;
    parser.finished();
    if parser.pos < parser.input.len() {
        return Err(ColumnFilterError::InvalidExpression);
    }
    Ok(expr)
}

fn eval_expr(expr: &Expr, value: f64) -> bool {
    match expr {
        Expr::Compare(op, rhs) => match op {
            CmpOp::Eq => (value - rhs).abs() < f64::EPSILON || value == *rhs,
            CmpOp::Ne => (value - rhs).abs() >= f64::EPSILON && value != *rhs,
            CmpOp::Gt => value > *rhs,
            CmpOp::Ge => value >= *rhs,
            CmpOp::Lt => value < *rhs,
            CmpOp::Le => value <= *rhs,
        },
        Expr::And(a, b) => eval_expr(a, value) && eval_expr(b, value),
        Expr::Or(a, b) => eval_expr(a, value) || eval_expr(b, value),
    }
}

fn parse_cell_number(cell: &str) -> Result<f64, ColumnFilterError> {
    let cell = cell.trim();
    if cell.is_empty() {
        return Err(ColumnFilterError::NonNumericCell);
    }
    cell.parse::<f64>()
        .map_err(|_| ColumnFilterError::NonNumericCell)
}

pub fn text_cell_matches(cell: &str, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    fuzzy_score(query, cell).is_some()
}

pub fn numeric_cell_matches(cell: &str, expr: &str) -> Result<bool, ColumnFilterError> {
    let parsed = parse_numeric_filter(expr)?;
    let value = parse_cell_number(cell)?;
    Ok(eval_expr(&parsed, value))
}

pub fn validate_numeric_filter(expr: &str) -> Result<(), ColumnFilterError> {
    parse_numeric_filter(expr).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_comparisons() {
        assert!(parse_numeric_filter(">10").is_ok());
        assert!(parse_numeric_filter("==0").is_ok());
        assert!(parse_numeric_filter("!=0").is_ok());
    }

    #[test]
    fn parses_compound_expressions() {
        let expr = parse_numeric_filter("(>=10) & (< 20)").unwrap();
        assert!(eval_expr(&expr, 15.0));
        assert!(!eval_expr(&expr, 5.0));
        assert!(!eval_expr(&expr, 25.0));
    }

    #[test]
    fn parses_or_expression() {
        let expr = parse_numeric_filter("(==0) | (== 1)").unwrap();
        assert!(eval_expr(&expr, 0.0));
        assert!(eval_expr(&expr, 1.0));
        assert!(!eval_expr(&expr, 2.0));
    }

    #[test]
    fn evaluates_against_cell() {
        assert!(numeric_cell_matches("15", "(>=10) & (< 20)").unwrap());
        assert!(numeric_cell_matches("abc", ">10").is_err());
    }

    #[test]
    fn fuzzy_text_match() {
        assert!(text_cell_matches("Tehran", "teh"));
        assert!(!text_cell_matches("Paris", "teh"));
    }
}
