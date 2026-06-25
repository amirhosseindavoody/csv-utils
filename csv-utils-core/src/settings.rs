use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

pub const SETTINGS_FILENAME: &str = "csv-utils.json";

pub fn default_numeric_decimal_format() -> String {
    ".3".to_string()
}

pub fn default_file_picker_extensions() -> Vec<String> {
    vec!["csv".to_string(), "dat".to_string()]
}

pub fn default_decimal_places() -> usize {
    3
}

pub fn default_show_column_borders() -> bool {
    true
}

/// Parse a format string like `.3` → 3 digits after the decimal point.
pub fn parse_decimal_format(s: &str) -> Option<usize> {
    let s = s.trim();
    if !s.starts_with('.') {
        return None;
    }
    let digits = &s[1..];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<usize>().ok().filter(|&n| n <= 12)
}

pub fn normalize_decimal_format(s: &str) -> Option<String> {
    parse_decimal_format(s).map(|n| format!(".{n}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsFile {
    #[serde(default)]
    pub display: DisplaySettings,
    #[serde(default)]
    pub file_picker: FilePickerSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePickerSettings {
    #[serde(default = "default_file_picker_extensions")]
    pub file_extensions: Vec<String>,
}

impl Default for FilePickerSettings {
    fn default() -> Self {
        Self {
            file_extensions: default_file_picker_extensions(),
        }
    }
}

impl FilePickerSettings {
    /// Lowercase extensions without a leading dot (e.g. `csv`, `dat`).
    pub fn normalized_extensions(&self) -> Vec<String> {
        self.file_extensions
            .iter()
            .map(|ext| ext.trim().trim_start_matches('.').to_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    #[serde(default = "default_numeric_decimal_format")]
    pub numeric_decimal_format: String,
    #[serde(default = "default_show_column_borders")]
    pub show_column_borders: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            numeric_decimal_format: default_numeric_decimal_format(),
            show_column_borders: default_show_column_borders(),
        }
    }
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            display: DisplaySettings::default(),
            file_picker: FilePickerSettings::default(),
        }
    }
}

impl SettingsFile {
    pub fn default_decimal_places(&self) -> usize {
        parse_decimal_format(&self.display.numeric_decimal_format)
            .unwrap_or_else(default_decimal_places)
    }
}

/// Load `csv-utils.json` from the current working directory, or create it with defaults.
pub fn load_or_create() -> io::Result<SettingsFile> {
    let path = Path::new(SETTINGS_FILENAME);
    if path.exists() {
        let data = fs::read_to_string(path)?;
        let settings: SettingsFile = serde_json::from_str(&data).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, e)
        })?;
        return Ok(settings);
    }

    let settings = SettingsFile::default();
    let data = serde_json::to_string_pretty(&settings).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, e)
    })?;
    fs::write(path, format!("{data}\n"))?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn parses_decimal_format() {
        assert_eq!(parse_decimal_format(".3"), Some(3));
        assert_eq!(parse_decimal_format(".0"), Some(0));
        assert_eq!(parse_decimal_format(".12"), Some(12));
        assert!(parse_decimal_format("3").is_none());
        assert!(parse_decimal_format(".x").is_none());
        assert!(parse_decimal_format(".").is_none());
    }

    #[test]
    fn creates_default_config_file() {
        let dir = TempDir::new().unwrap();
        let prev = env::current_dir().unwrap();
        env::set_current_dir(dir.path()).unwrap();
        let settings = load_or_create().unwrap();
        assert_eq!(settings.display.numeric_decimal_format, ".3");
        assert!(settings.display.show_column_borders);
        assert_eq!(
            settings.file_picker.file_extensions,
            vec!["csv".to_string(), "dat".to_string()]
        );
        assert!(dir.path().join(SETTINGS_FILENAME).exists());
        env::set_current_dir(prev).unwrap();
    }

    #[test]
    fn normalizes_file_picker_extensions() {
        let settings = FilePickerSettings {
            file_extensions: vec![".CSV".into(), " dat ".into(), "".into()],
        };
        assert_eq!(
            settings.normalized_extensions(),
            vec!["csv".to_string(), "dat".to_string()]
        );
    }
}
