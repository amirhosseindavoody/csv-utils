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

/// Syntax token kinds for pretty-printed JSON highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonTokenKind {
    Key,
    String,
    Number,
    Literal,
    Punctuation,
    Whitespace,
    Other,
}

/// Highlight a single line of JSON into styled token spans.
///
/// Quoted strings immediately followed (after optional whitespace) by `:` are
/// treated as object keys; other quoted strings are values.
pub fn highlight_json_line(line: &str) -> Vec<(JsonTokenKind, &str)> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            tokens.push((JsonTokenKind::Whitespace, &line[start..i]));
            continue;
        }
        if b == b'"' {
            i = scan_json_string(bytes, i);
            let text = &line[start..i];
            let kind = if is_object_key_after(bytes, i) {
                JsonTokenKind::Key
            } else {
                JsonTokenKind::String
            };
            tokens.push((kind, text));
            continue;
        }
        if b == b'-' || b.is_ascii_digit() {
            i = scan_json_number(bytes, i);
            tokens.push((JsonTokenKind::Number, &line[start..i]));
            continue;
        }
        if matches!(b, b'{' | b'}' | b'[' | b']' | b':' | b',') {
            i += 1;
            tokens.push((JsonTokenKind::Punctuation, &line[start..i]));
            continue;
        }
        if line[i..].starts_with("true")
            || line[i..].starts_with("false")
            || line[i..].starts_with("null")
        {
            let lit = if line[i..].starts_with("true") {
                4
            } else if line[i..].starts_with("false") {
                5
            } else {
                4
            };
            i += lit;
            tokens.push((JsonTokenKind::Literal, &line[start..i]));
            continue;
        }
        // Fallback: take one char (UTF-8 safe).
        i += line[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        tokens.push((JsonTokenKind::Other, &line[start..i]));
    }
    tokens
}

/// Highlight a full JSON document line-by-line.
pub fn highlight_json(text: &str) -> Vec<Vec<(JsonTokenKind, String)>> {
    text.lines()
        .map(|line| {
            highlight_json_line(line)
                .into_iter()
                .map(|(kind, s)| (kind, s.to_string()))
                .collect()
        })
        .collect()
}

fn scan_json_string(bytes: &[u8], mut i: usize) -> usize {
    // Caller starts on opening quote.
    i += 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                break;
            }
            _ => i += 1,
        }
    }
    i
}

fn scan_json_number(bytes: &[u8], mut i: usize) -> usize {
    if bytes.get(i) == Some(&b'-') {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if matches!(bytes.get(i), Some(&b'e') | Some(&b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(&b'+') | Some(&b'-')) {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    i
}

fn is_object_key_after(bytes: &[u8], mut i: usize) -> bool {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    bytes.get(i) == Some(&b':')
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

    #[test]
    fn highlight_distinguishes_keys_and_string_values() {
        let line = r#"  "id": "Ada","#;
        let tokens = highlight_json_line(line);
        let kinds: Vec<_> = tokens
            .iter()
            .filter(|(k, t)| *k != JsonTokenKind::Whitespace && !t.is_empty())
            .map(|(k, t)| (*k, *t))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (JsonTokenKind::Key, r#""id""#),
                (JsonTokenKind::Punctuation, ":"),
                (JsonTokenKind::String, r#""Ada""#),
                (JsonTokenKind::Punctuation, ","),
            ]
        );
    }

    #[test]
    fn highlight_numbers_and_literals() {
        let line = r#"  "n": 12.5, "ok": true, "x": null"#;
        let tokens = highlight_json_line(line);
        assert!(tokens.iter().any(|(k, t)| *k == JsonTokenKind::Number && *t == "12.5"));
        assert!(tokens.iter().any(|(k, t)| *k == JsonTokenKind::Literal && *t == "true"));
        assert!(tokens.iter().any(|(k, t)| *k == JsonTokenKind::Literal && *t == "null"));
        assert!(tokens.iter().any(|(k, t)| *k == JsonTokenKind::Key && *t == r#""n""#));
    }

    #[test]
    fn highlight_escaped_quotes_inside_strings() {
        let line = r#"  "msg": "he said \"hi\"","#;
        let tokens = highlight_json_line(line);
        assert!(tokens
            .iter()
            .any(|(k, t)| *k == JsonTokenKind::String && t.contains(r#"\"hi\""#)));
        assert!(tokens.iter().any(|(k, t)| *k == JsonTokenKind::Key && *t == r#""msg""#));
    }
}
