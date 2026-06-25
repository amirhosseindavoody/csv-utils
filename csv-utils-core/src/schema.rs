use csv::ByteRecord;
use std::io;

/// Parse a single CSV record from a byte slice (RFC 4180 via the `csv` crate).
pub fn read_fields_from_slice(data: &[u8]) -> io::Result<Vec<String>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(data);
    let mut record = ByteRecord::new();
    if reader.read_byte_record(&mut record)? {
        Ok(fields_from_byte_record(&record))
    } else {
        Ok(Vec::new())
    }
}

pub fn fields_from_byte_record(record: &ByteRecord) -> Vec<String> {
    record
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}

/// Split one line of CSV text into fields (CLI path; one record per line).
pub fn split_row(line: &str) -> Vec<String> {
    read_fields_from_slice(line.trim_end_matches('\r').as_bytes()).unwrap_or_default()
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

    #[test]
    fn parses_embedded_newline_in_quoted_field() {
        let data = br#""a
b",c"#;
        let fields = read_fields_from_slice(data).unwrap();
        assert_eq!(fields, vec!["a\nb", "c"]);
    }
}
