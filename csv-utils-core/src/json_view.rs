use serde_json::{Map, Value};
use std::io::Write;

/// Compact one-line JSON object for CLI output (`csv json` / `filter`).
pub fn format_row(headers: &[String], fields: &[String]) -> String {
    let mut out = format_row_value(headers, fields).to_string();
    out.push('\n');
    out
}

/// Pretty-printed JSON object for the TUI/web row panel.
pub fn format_row_pretty(headers: &[String], fields: &[String]) -> String {
    serde_json::to_string_pretty(&format_row_value(headers, fields)).unwrap_or_else(|_| "{\n}".into())
}

fn format_row_value(headers: &[String], fields: &[String]) -> Value {
    let limit = headers.len().min(fields.len());
    let mut map = Map::new();
    for i in 0..limit {
        map.insert(headers[i].clone(), Value::String(fields[i].clone()));
    }
    Value::Object(map)
}

pub fn print_row(headers: &[String], fields: &[String], mut out: impl Write) -> std::io::Result<()> {
    write!(out, "{}", format_row(headers, fields).trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_and_backslashes() {
        let headers = vec!["a".into(), "b".into()];
        let fields = vec!["he said \"hi\"".into(), "c:\\x".into()];
        let compact = format_row(&headers, &fields);
        assert!(compact.contains(r#""a":"he said \"hi\""#));
        assert!(compact.contains(r#""b":"c:\\x""#));
    }

    #[test]
    fn pretty_spans_multiple_lines() {
        let headers = vec!["id".into(), "name".into()];
        let fields = vec!["1".into(), "Ada".into()];
        let pretty = format_row_pretty(&headers, &fields);
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("\"id\""));
        assert!(pretty.contains("\"Ada\""));
    }
}
