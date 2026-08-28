use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Runtime configuration loaded from defaults, environment, and optional config.json.
///
/// Secrets and provider settings are intentionally absent in Phase 1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub app_name: String,
    pub version: String,
    pub log_level: String,
    pub app_config_dir: Option<PathBuf>,
    pub app_data_dir: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    log_level: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        Self::new(std::env::var("COMBAIN_STUDIO_LOG_LEVEL").ok())
    }

    pub fn new(log_level: Option<String>) -> Result<Self, AppError> {
        let log_level = match log_level {
            Some(value) if !value.trim().is_empty() => normalize_log_level(&value)?,
            _ => "info".to_string(),
        };

        Ok(Self {
            app_name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            log_level,
            app_config_dir: None,
            app_data_dir: None,
        })
    }

    /// Merge an optional JSON file. Missing files are ignored.
    pub fn merge_file(&mut self, path: &Path) -> Result<(), AppError> {
        if !path.exists() {
            return Ok(());
        }

        let raw = std::fs::read_to_string(path)
            .map_err(|error| AppError::io(format!("failed to read {}: {error}", path.display())))?;
        let file: ConfigFile = serde_json::from_str(&raw).map_err(|error| {
            AppError::config(format!("invalid config file {}: {error}", path.display()))
        })?;

        if let Some(log_level) = file.log_level {
            self.log_level = normalize_log_level(&log_level)?;
        }

        Ok(())
    }
}

fn normalize_log_level(value: &str) -> Result<String, AppError> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "error" | "warn" | "info" | "debug" | "trace" => Ok(normalized),
        other => Err(AppError::invalid_input(format!(
            "unsupported log_level '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn defaults_to_info_log_level() {
        let config = AppConfig::new(None).expect("defaults");
        assert_eq!(config.app_name, "combain_studio");
        assert_eq!(config.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn rejects_invalid_log_level() {
        let error = AppConfig::new(Some("verbose".into())).expect_err("invalid level");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn merges_config_file_log_level() {
        let dir = std::env::temp_dir().join(format!(
            "combain-studio-config-{}-{}",
            std::process::id(),
            "merge"
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.json");
        fs::write(&path, r#"{"log_level":"DEBUG"}"#).expect("write config");

        let mut config = AppConfig::new(None).expect("defaults");
        config.merge_file(&path).expect("merge");
        assert_eq!(config.log_level, "debug");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_config_file_is_ok() {
        let mut config = AppConfig::new(None).expect("defaults");
        config
            .merge_file(Path::new("definitely-missing-combain-studio-config.json"))
            .expect("missing file");
        assert_eq!(config.log_level, "info");
    }
}
