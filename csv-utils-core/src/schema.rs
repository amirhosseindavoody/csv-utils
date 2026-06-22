/// Split a CSV line into fields using the same rules as the Zig implementation:
/// quoted fields, `""` escape, comma split outside quotes, trailing `\r` trimmed.
pub fn split_row(line: &str) -> Vec<String> {
    let trimmed = line.trim_end_matches('\r');
    let mut storage = trimmed.as_bytes().to_vec();
    let mut fields: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut field_start: usize = 0;
    let mut write_idx: usize = 0;
    let mut i = 0;

    while i < storage.len() {
        let ch = storage[i];
        if ch == b'"' {
            if in_quotes && i + 1 < storage.len() && storage[i + 1] == b'"' {
                storage[write_idx] = b'"';
                write_idx += 1;
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }

        if ch == b',' && !in_quotes {
            fields.push(String::from_utf8_lossy(&storage[field_start..write_idx]).into_owned());
            field_start = write_idx + 1;
            storage[write_idx] = b',';
            write_idx += 1;
            i += 1;
            continue;
        }

        storage[write_idx] = ch;
        write_idx += 1;
        i += 1;
    }

    fields.push(String::from_utf8_lossy(&storage[field_start..write_idx]).into_owned());
    fields
}

pub fn index_of(headers: &[String], name: &str) -> Option<usize> {
    headers.iter().position(|h| h == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_fields() {
        let fields = split_row(r#""a,b",c"#);
        assert_eq!(fields, vec!["a,b", "c"]);
    }

    #[test]
    fn unescapes_double_quotes() {
        let fields = split_row(r#""say ""hi""",ok"#);
        assert_eq!(fields, vec![r#"say "hi""#, "ok"]);
    }
}
