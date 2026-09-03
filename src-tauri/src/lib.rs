mod commands;
mod config;
mod db;
mod domain;
mod error;
mod logging;
mod paths;
mod provider;
mod repository;
mod secrets;
mod storage;

use config::AppConfig;
use db::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_config = match AppConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load configuration: {error}");
            std::process::exit(1);
        }
    };

    logging::init(&startup_config.log_level);
    tracing::info!(
        app = %startup_config.app_name,
        version = %startup_config.version,
        log_level = %startup_config.log_level,
        "starting Combain Studio"
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::greet,
            commands::project_commands::create_project,
            commands::project_commands::get_project,
            commands::project_commands::list_projects,
            commands::project_commands::update_project,
            commands::scene_commands::get_scenes,
            commands::scene_commands::create_scene,
            commands::scene_commands::update_scene,
            commands::scene_commands::delete_scene,
            commands::scene_commands::reorder_scenes,
            commands::script_commands::generate_script,
            commands::asset_commands::generate_image,
            commands::asset_commands::generate_video,
            commands::asset_commands::get_assets,
            commands::asset_commands::get_asset_versions,
            commands::asset_commands::select_asset_version,
            commands::provider_profile_commands::list_provider_profiles,
            commands::provider_profile_commands::create_provider_profile,
            commands::provider_profile_commands::update_provider_profile,
            commands::provider_profile_commands::delete_provider_profile,
        ])
        .setup(move |app| {
            let mut config = startup_config;

            match app.path().app_config_dir() {
                Ok(dir) => {
                    let file = dir.join("config.json");
                    match config.merge_file(&file) {
                        Ok(()) => tracing::debug!(path = %file.display(), "optional config file considered"),
                        Err(error) => {
                            tracing::warn!(%error, path = %file.display(), "optional config file ignored")
                        }
                    }
                    config.app_config_dir = Some(dir);
                }
                Err(error) => tracing::warn!(%error, "app config directory unavailable"),
            }

            let data_dir = paths::portable_data_dir().map_err(|error| {
                tracing::error!(%error, "portable data directory unavailable");
                error
            })?;
            config.app_data_dir = Some(data_dir.clone());

            let generated_projects_dir = data_dir.join("projects");
            app.asset_protocol_scope()
                .allow_directory(&generated_projects_dir, true)
                .map_err(|error| {
                    tracing::error!(
                        %error,
                        path = %generated_projects_dir.display(),
                        "failed to allow generated project assets in asset protocol scope"
                    );
                    error
                })?;
            tracing::info!(
                path = %generated_projects_dir.display(),
                "generated project assets allowed in asset protocol scope"
            );

            tracing::info!(
                log_level = %config.log_level,
                config_dir = ?config.app_config_dir,
                data_dir = ?config.app_data_dir,
                "configuration ready"
            );

            let db_path = data_dir.join("database.sqlite");
            let database = Database::open(&db_path).map_err(|error| {
                tracing::error!(%error, path = %db_path.display(), "failed to open database");
                error
            })?;
            tracing::info!(path = %db_path.display(), "database ready");

            app.manage(config);
            app.manage(database);
            tracing::info!("Rust backend ready");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
