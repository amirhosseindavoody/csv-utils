use crate::schema;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

const READ_BUF_SIZE: usize = 1024 * 1024;
pub const INITIAL_BODY_LINES: usize = 128;

#[derive(Debug)]
pub struct PreviewData {
    inner: Arc<Mutex<PreviewInner>>,
}

#[derive(Debug)]
    pub(crate) struct PreviewInner {
    headers: Vec<String>,
    rows: Vec<String>,
    scan_done: bool,
    scan_error: bool,
    bytes_loaded: usize,
}

impl PreviewData {
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PreviewInner {
                headers: Vec::new(),
                rows: Vec::new(),
                scan_done: true,
                scan_error: false,
                bytes_loaded: 0,
            })),
        }
    }

    pub fn load_header_and_initial_rows(path: &Path, initial_body_lines: usize) -> std::io::Result<Self> {
        load_from_path(path, initial_body_lines, false)
    }

    pub fn load_limited(path: &Path, limit: usize) -> std::io::Result<Self> {
        load_from_path(path, limit, true)
    }

    pub fn start_background_scan(&self, path: &Path, skip_body_lines: usize) -> thread::JoinHandle<()> {
        let data = Arc::clone(&self.inner);
        let path = path.to_path_buf();
        thread::spawn(move || stream_append_body_lines_after_skip(&data, &path, skip_body_lines))
    }

    fn with_read_lock<R>(&self, f: impl FnOnce(&PreviewInner) -> R) -> R {
        let guard = self.inner.lock().expect("preview mutex poisoned");
        f(&guard)
    }

    pub fn row_count(&self) -> usize {
        self.with_read_lock(|inner| inner.rows.len())
    }

    pub fn headers(&self) -> Vec<String> {
        self.with_read_lock(|inner| inner.headers.clone())
    }

    pub fn scan_done(&self) -> bool {
        self.with_read_lock(|inner| inner.scan_done)
    }

    pub fn scan_error(&self) -> bool {
        self.with_read_lock(|inner| inner.scan_error)
    }

    pub fn row_line(&self, index: usize) -> Option<String> {
        self.with_read_lock(|inner| inner.rows.get(index).cloned())
    }
}

fn load_from_path(path: &Path, max_body_lines: usize, scan_done_flag: bool) -> std::io::Result<PreviewData> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(READ_BUF_SIZE, file);
    let mut line = String::new();

    if reader.read_line(&mut line)? == 0 {
        return Ok(PreviewData::empty());
    }
    let headers = parse_header_row(line.trim_end())?;
    line.clear();

    let mut rows = Vec::new();
    let mut bytes_loaded = 0usize;
    while rows.len() < max_body_lines {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let owned = line.trim_end_matches('\n').to_string();
        bytes_loaded += owned.len();
        rows.push(owned);
    }

    Ok(PreviewData {
        inner: Arc::new(Mutex::new(PreviewInner {
            headers,
            rows,
            scan_done: scan_done_flag,
            scan_error: false,
            bytes_loaded,
        })),
    })
}

fn parse_header_row(header_slice: &str) -> std::io::Result<Vec<String>> {
    Ok(schema::split_row(header_slice))
}

fn stream_append_body_lines_after_skip(data: &Arc<Mutex<PreviewInner>>, path: &Path, skip_body_lines: usize) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => {
            mark_scan_error(data);
            return;
        }
    };
    let mut reader = BufReader::with_capacity(READ_BUF_SIZE, file);
    let mut line = String::new();

    if reader.read_line(&mut line).is_err() {
        mark_scan_error(data);
        return;
    }
    line.clear();

    let mut skipped = 0usize;
    while skipped < skip_body_lines {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                mark_scan_done(data);
                return;
            }
            Ok(_) => skipped += 1,
            Err(_) => {
                mark_scan_error(data);
                return;
            }
        }
    }

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let owned = line.trim_end_matches('\n').to_string();
                let mut guard = data.lock().expect("preview mutex poisoned");
                guard.bytes_loaded += owned.len();
                guard.rows.push(owned);
            }
            Err(_) => {
                let mut guard = data.lock().expect("preview mutex poisoned");
                guard.scan_error = true;
                break;
            }
        }
    }

    mark_scan_done(data);
}

fn mark_scan_error(data: &Arc<Mutex<PreviewInner>>) {
    let mut guard = data.lock().expect("preview mutex poisoned");
    guard.scan_error = true;
    guard.scan_done = true;
}

fn mark_scan_done(data: &Arc<Mutex<PreviewInner>>) {
    let mut guard = data.lock().expect("preview mutex poisoned");
    guard.scan_done = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn loads_header_and_rows() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "a,b").unwrap();
        writeln!(file, "1,2").unwrap();
        writeln!(file, "3,4").unwrap();
        file.flush().unwrap();

        let preview = PreviewData::load_limited(file.path(), 2).unwrap();
        assert_eq!(preview.headers(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(preview.row_count(), 2);
        assert!(preview.scan_done());
    }
}
