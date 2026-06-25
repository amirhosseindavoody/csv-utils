use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const SETTINGS_FILENAME: &str = "csv-utils.json";
pub const SETTINGS_DIR_NAME: &str = "csv-utils";

/// Override global config directory in tests (`CSV_UTILS_CONFIG_DIR`).
pub const CONFIG_DIR_ENV: &str = "CSV_UTILS_CONFIG_DIR";

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsFile {
    #[serde(default)]
    pub display: DisplaySettings,
    #[serde(default)]
    pub file_picker: FilePickerSettings,
}

#[derive(Debug, Clone)]
pub struct LoadedSettings {
    pub settings: SettingsFile,
    pub global_path: PathBuf,
    pub local_path: PathBuf,
    pub global_created: bool,
    pub local_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

/// User config directory: `$CSV_UTILS_CONFIG_DIR`, else `$XDG_CONFIG_HOME/csv-utils`,
/// else `~/.config/csv-utils`.
pub fn global_settings_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(CONFIG_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join(SETTINGS_DIR_NAME));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join(SETTINGS_DIR_NAME))
}

pub fn global_settings_path() -> Option<PathBuf> {
    global_settings_dir().map(|dir| dir.join(SETTINGS_FILENAME))
}

/// Project-local override path: `./csv-utils.json` in the process working directory.
pub fn local_settings_path() -> PathBuf {
    PathBuf::from(SETTINGS_FILENAME)
}

fn read_settings_file(path: &Path) -> io::Result<SettingsFile> {
    let data = fs::read_to_string(path)?;
    serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_settings_file(path: &Path, settings: &SettingsFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let data = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, format!("{data}\n"))
}

/// Load global settings from the user config directory, creating the file with defaults when absent.
pub fn load_or_create_global(path: &Path) -> io::Result<(SettingsFile, bool)> {
    if path.exists() {
        return read_settings_file(path).map(|settings| (settings, false));
    }
    let settings = SettingsFile::default();
    write_settings_file(path, &settings)?;
    Ok((settings, true))
}

/// Deep-merge `overlay` into `base` (objects recurse; other values replace).
pub fn merge_json_values(base: &mut serde_json::Value, overlay: serde_json::Value) {
    let serde_json::Value::Object(overlay_map) = overlay else {
        return;
    };
    let serde_json::Value::Object(base_map) = base else {
        *base = serde_json::Value::Object(overlay_map);
        return;
    };
    for (key, value) in overlay_map {
        match (base_map.get_mut(&key), value) {
            (Some(base_slot), serde_json::Value::Object(overlay_child)) if base_slot.is_object() => {
                merge_json_values(base_slot, serde_json::Value::Object(overlay_child));
            }
            (Some(base_slot), overlay_value) => {
                *base_slot = overlay_value;
            }
            (None, overlay_value) => {
                base_map.insert(key, overlay_value);
            }
        }
    }
}

/// Merge a local settings file on top of global settings (field-level / deep object merge).
pub fn merge_settings(global: &SettingsFile, local_path: &Path) -> io::Result<SettingsFile> {
    let local_data = fs::read_to_string(local_path)?;
    let local_value: serde_json::Value = serde_json::from_str(&local_data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut merged_value = serde_json::to_value(global)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    merge_json_values(&mut merged_value, local_value);
    serde_json::from_value(merged_value).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Load layered settings: global (home config dir) with optional local (`./csv-utils.json`) overrides.
pub fn load_merged() -> LoadedSettings {
    let local_path = local_settings_path();
    let global_path = global_settings_path().unwrap_or_else(|| local_path.clone());

    let (mut settings, global_created) = load_or_create_global(&global_path).unwrap_or_else(|_| {
        (SettingsFile::default(), false)
    });

    let mut local_applied = false;
    if local_path.exists() && local_path != global_path {
        if let Ok(merged) = merge_settings(&settings, &local_path) {
            settings = merged;
            local_applied = true;
        }
    }

    LoadedSettings {
        settings,
        global_path,
        local_path,
        global_created,
        local_applied,
    }
}

/// Load merged settings, returning only the resolved `SettingsFile`.
pub fn load_or_create() -> io::Result<SettingsFile> {
    Ok(load_merged().settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    fn with_config_dir<F>(f: F)
    where
        F: FnOnce(&Path),
    {
        let dir = TempDir::new().unwrap();
        let prev = env::var(CONFIG_DIR_ENV).ok();
        env::set_var(CONFIG_DIR_ENV, dir.path());
        f(dir.path());
        match prev {
            Some(value) => env::set_var(CONFIG_DIR_ENV, value),
            None => env::remove_var(CONFIG_DIR_ENV),
        }
    }

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
    fn creates_global_config_file() {
        with_config_dir(|config_dir| {
            let path = config_dir.join(SETTINGS_FILENAME);
            let (settings, created) = load_or_create_global(&path).unwrap();
            assert!(created);
            assert_eq!(settings.display.numeric_decimal_format, ".3");
            assert!(settings.display.show_column_borders);
            assert_eq!(
                settings.file_picker.file_extensions,
                vec!["csv".to_string(), "dat".to_string()]
            );
            assert!(path.exists());
        });
    }

    #[test]
    fn local_overrides_global_fields() {
        with_config_dir(|config_dir| {
            let global_path = config_dir.join(SETTINGS_FILENAME);
            write_settings_file(
                &global_path,
                &SettingsFile {
                    display: DisplaySettings {
                        numeric_decimal_format: ".2".to_string(),
                        show_column_borders: true,
                    },
                    file_picker: FilePickerSettings::default(),
                },
            )
            .unwrap();

            let work = TempDir::new().unwrap();
            let local_path = work.path().join(SETTINGS_FILENAME);
            fs::write(
                &local_path,
                r#"{
  "display": {
    "numeric_decimal_format": ".5"
  },
  "file_picker": {
    "file_extensions": ["tsv"]
  }
}
"#,
            )
            .unwrap();

            let prev = env::current_dir().unwrap();
            env::set_current_dir(work.path()).unwrap();
            let loaded = load_merged();
            env::set_current_dir(prev).unwrap();

            assert_eq!(loaded.settings.display.numeric_decimal_format, ".5");
            assert!(loaded.settings.display.show_column_borders);
            assert_eq!(
                loaded.settings.file_picker.file_extensions,
                vec!["tsv".to_string()]
            );
            assert!(loaded.local_applied);
        });
    }

    #[test]
    fn invalid_local_file_is_ignored() {
        with_config_dir(|config_dir| {
            let global_path = config_dir.join(SETTINGS_FILENAME);
            write_settings_file(
                &global_path,
                &SettingsFile {
                    display: DisplaySettings {
                        numeric_decimal_format: ".4".to_string(),
                        ..DisplaySettings::default()
                    },
                    ..SettingsFile::default()
                },
            )
            .unwrap();

            let work = TempDir::new().unwrap();
            fs::write(work.path().join(SETTINGS_FILENAME), "{ not json").unwrap();

            let prev = env::current_dir().unwrap();
            env::set_current_dir(work.path()).unwrap();
            let loaded = load_merged();
            env::set_current_dir(prev).unwrap();

            assert_eq!(loaded.settings.display.numeric_decimal_format, ".4");
            assert!(!loaded.local_applied);
        });
    }

    #[test]
    fn merge_json_values_recurses_objects() {
        let mut base = serde_json::json!({
            "display": { "numeric_decimal_format": ".2", "show_column_borders": true },
            "file_picker": { "file_extensions": ["csv"] }
        });
        let overlay = serde_json::json!({
            "display": { "numeric_decimal_format": ".7" }
        });
        merge_json_values(&mut base, overlay);
        assert_eq!(base["display"]["numeric_decimal_format"], ".7");
        assert_eq!(base["display"]["show_column_borders"], true);
        assert_eq!(base["file_picker"]["file_extensions"], serde_json::json!(["csv"]));
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
