mod auth;
mod commands;
mod download;
mod error;
mod instance;
mod java;
mod launch;
mod loader;
mod metadata;
mod mods;
mod paths;
mod settings;
mod state;

use std::process::ExitCode;

use paths::AppPaths;
use settings::Settings;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> ExitCode {
    let paths = match AppPaths::resolve() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("failed to resolve application paths: {err}");
            return ExitCode::FAILURE;
        }
    };
    let settings = match Settings::load(&paths) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("failed to load settings: {err}");
            return ExitCode::FAILURE;
        }
    };
    let app_state = AppState::new(paths, settings);

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_versions,
            commands::list_instances,
            commands::create_instance,
            commands::delete_instance,
            commands::list_loader_versions,
            commands::install_instance,
            commands::cancel_install,
            commands::search_mods,
            commands::install_mod,
            commands::list_installed_mods,
            commands::remove_mod,
            commands::login,
            commands::login_offline,
            commands::restore_session,
            commands::logout,
            commands::get_account,
            commands::list_java,
            commands::launch_instance,
            commands::open_azure_setup,
            commands::open_mojang_app_review,
            commands::data_paths,
        ])
        .run(tauri::generate_context!());

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error while running tauri application: {err}");
            ExitCode::FAILURE
        }
    }
}
