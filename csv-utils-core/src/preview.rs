use crate::column_layout::ColumnLayoutState;
use crate::schema::{fields_from_byte_record, read_fields_from_slice};
use memmap2::{Mmap, MmapOptions};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

pub const INITIAL_BODY_LINES: usize = 128;

#[derive(Debug)]
pub struct PreviewData {
    inner: Arc<Mutex<PreviewInner>>,
    layout: Arc<Mutex<ColumnLayoutState>>,
}

#[derive(Debug)]
struct PreviewInner {
    mmap: Option<Arc<Mmap>>,
    headers: Vec<String>,
    record_offsets: Vec<u64>,
    scan_done: bool,
    scan_error: bool,
}

impl PreviewData {
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PreviewInner {
                mmap: None,
                headers: Vec::new(),
                record_offsets: Vec::new(),
                scan_done: true,
                scan_error: false,
            })),
            layout: Arc::new(Mutex::new(ColumnLayoutState::default())),
        }
    }

    pub fn load_header_and_initial_rows(path: &Path, initial_body_lines: usize) -> std::io::Result<Self> {
        load_from_path(path, initial_body_lines, false)
    }

    pub fn load_limited(path: &Path, limit: usize) -> std::io::Result<Self> {
        load_from_path(path, limit, true)
    }

    pub fn start_background_scan(&self, path: &Path, skip_body_records: usize) -> thread::JoinHandle<()> {
        let data = Arc::clone(&self.inner);
        let layout = Arc::clone(&self.layout);
        let path = path.to_path_buf();
        thread::spawn(move || continue_background_scan(&data, &layout, &path, skip_body_records))
    }

    pub fn layout(&self) -> Arc<Mutex<ColumnLayoutState>> {
        Arc::clone(&self.layout)
    }

    fn with_read_lock<R>(&self, f: impl FnOnce(&PreviewInner) -> R) -> R {
        let guard = self.inner.lock().expect("preview mutex poisoned");
        f(&guard)
    }

    pub fn row_count(&self) -> usize {
        self.with_read_lock(|inner| inner.record_offsets.len())
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

    pub fn row_fields(&self, index: usize) -> Option<Vec<String>> {
        self.with_read_lock(|inner| row_fields_from_inner(inner, index))
    }
}

fn row_fields_from_inner(inner: &PreviewInner, index: usize) -> Option<Vec<String>> {
    let mmap = inner.mmap.as_ref()?;
    let start = *inner.record_offsets.get(index)? as usize;
    let end = inner
        .record_offsets
        .get(index + 1)
        .map(|&offset| offset as usize)
        .unwrap_or(mmap.len());
    read_fields_from_slice(&mmap[start..end]).ok()
}

fn load_from_path(path: &Path, max_body_records: usize, force_scan_done: bool) -> std::io::Result<PreviewData> {
    let file = File::open(path)?;
    let mmap = Arc::new(unsafe { MmapOptions::new().map(&file)? });

    if mmap.is_empty() {
        return Ok(PreviewData::empty());
    }

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(&mmap[..]);
    let mut record = csv::ByteRecord::new();

    if !reader.read_byte_record(&mut record)? {
        return Ok(PreviewData::empty());
    }

    let headers = fields_from_byte_record(&record);
    let layout = Arc::new(Mutex::new(ColumnLayoutState::default()));
    layout.lock().expect("layout mutex poisoned").reset_from_headers(&headers);

    let mut record_offsets = Vec::new();
    let mut body_count = 0usize;
    let mut hit_eof = false;

    while body_count < max_body_records {
        let offset = reader.position().byte();
        if !reader.read_byte_record(&mut record)? {
            hit_eof = true;
            break;
        }
        record_offsets.push(offset);
        let fields = fields_from_byte_record(&record);
        layout
            .lock()
            .expect("layout mutex poisoned")
            .observe_fields(&fields);
        body_count += 1;
    }

    let scan_done = force_scan_done || hit_eof;

    Ok(PreviewData {
        inner: Arc::new(Mutex::new(PreviewInner {
            mmap: Some(mmap),
            headers,
            record_offsets,
            scan_done,
            scan_error: false,
        })),
        layout,
    })
}

fn continue_background_scan(
    data: &Arc<Mutex<PreviewInner>>,
    layout: &Arc<Mutex<ColumnLayoutState>>,
    path: &Path,
    skip_body_records: usize,
) {
    let mmap = match map_file(path) {
        Ok(m) => m,
        Err(_) => {
            mark_scan_error(data);
            return;
        }
    };

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(&mmap[..]);
    let mut record = csv::ByteRecord::new();

    if reader.read_byte_record(&mut record).is_err() {
        mark_scan_error(data);
        return;
    }

    for _ in 0..skip_body_records {
        match reader.read_byte_record(&mut record) {
            Ok(true) => {}
            Ok(false) => {
                mark_scan_done(data);
                return;
            }
            Err(_) => {
                mark_scan_error(data);
                return;
            }
        }
    }

    loop {
        let offset = reader.position().byte();
        match reader.read_byte_record(&mut record) {
            Ok(true) => {
                let fields = fields_from_byte_record(&record);
                {
                    let mut guard = data.lock().expect("preview mutex poisoned");
                    guard.record_offsets.push(offset);
                }
                layout
                    .lock()
                    .expect("layout mutex poisoned")
                    .observe_fields(&fields);
            }
            Ok(false) => break,
            Err(_) => {
                mark_scan_error(data);
                return;
            }
        }
    }

    mark_scan_done(data);
}

fn map_file(path: &Path) -> std::io::Result<Arc<Mmap>> {
    let file = File::open(path)?;
    Ok(Arc::new(unsafe { MmapOptions::new().map(&file)? }))
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
        assert_eq!(preview.row_fields(0), Some(vec!["1".to_string(), "2".to_string()]));
        assert!(preview.scan_done());
    }

    #[test]
    fn parses_quoted_embedded_newline() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "a,b\n\"1\n2\",3\n").unwrap();
        file.flush().unwrap();

        let preview = PreviewData::load_limited(file.path(), 10).unwrap();
        assert_eq!(preview.row_count(), 1);
        assert_eq!(
            preview.row_fields(0),
            Some(vec!["1\n2".to_string(), "3".to_string()])
        );
    }
}
