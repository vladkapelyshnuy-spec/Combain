pub mod project_commands;
pub mod scene_commands;

use serde::Serialize;
use tauri::State;

use crate::config::AppConfig;
use crate::error::AppError;

/// Response for the Phase 1 connectivity check.
///
/// Input: none  
/// Output: [`PingResponse`]  
/// Error: [`AppError`]  
/// Side effects: none (logs only)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PingResponse {
    pub ok: bool,
    pub app: String,
    pub version: String,
}

pub fn build_ping_response(config: &AppConfig) -> PingResponse {
    PingResponse {
        ok: true,
        app: config.app_name.clone(),
        version: config.version.clone(),
    }
}

#[tauri::command]
pub fn ping(config: State<'_, AppConfig>) -> Result<PingResponse, AppError> {
    tracing::info!(app = %config.app_name, version = %config.version, "ipc ping");
    Ok(build_ping_response(&config))
}

/// Kept so the scaffold frontend (`invoke("greet")`) continues to work.
#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_app_identity() {
        let config = AppConfig {
            app_name: "combain_studio".into(),
            version: "0.1.0".into(),
            log_level: "info".into(),
            app_config_dir: None,
            app_data_dir: None,
        };

        let response = build_ping_response(&config);
        assert!(response.ok);
        assert_eq!(response.app, "combain_studio");
        assert_eq!(response.version, "0.1.0");
    }
}
