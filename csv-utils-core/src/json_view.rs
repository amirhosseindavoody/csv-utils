use std::io::Write;

pub fn format_row(headers: &[String], fields: &[String]) -> String {
    let limit = headers.len().min(fields.len());
    let mut out = String::from("{");
    for i in 0..limit {
        if i != 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("\"{}\": \"{}\"", headers[i], fields[i]));
    }
    out.push_str("}\n");
    out
}

pub fn print_row(headers: &[String], fields: &[String], mut out: impl Write) -> std::io::Result<()> {
    write!(out, "{}", format_row(headers, fields).trim_end())
}
