use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::auth::{self, AccountInfo};
use crate::error::{AppError, AppResult};
use crate::instance::{self, CreateInstanceRequest, Instance, ModLoader};
use crate::java::{self, JavaRuntime};
use crate::loader::{self, LoaderVersionInfo};
use crate::metadata::{self, VersionInfo};
use crate::mods::{self, ModHit};
use crate::settings::Settings;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<()> {
    settings.save(&state.paths)?;
    *state.settings.lock() = settings;
    Ok(())
}

#[tauri::command]
pub async fn list_versions() -> AppResult<Vec<VersionInfo>> {
    let manifest = metadata::fetch_version_manifest().await?;
    Ok(manifest.versions)
}

#[tauri::command]
pub fn list_instances(state: State<'_, AppState>) -> AppResult<Vec<Instance>> {
    instance::list_instances(&state.paths)
}

#[tauri::command]
pub fn create_instance(
    state: State<'_, AppState>,
    request: CreateInstanceRequest,
) -> AppResult<Instance> {
    let created = instance::create_instance(&state.paths, request)?;
    let mut settings = state.settings.lock().clone();
    settings.selected_instance_id = Some(created.id.clone());
    settings.save(&state.paths)?;
    *state.settings.lock() = settings;
    Ok(created)
}

#[tauri::command]
pub fn delete_instance(state: State<'_, AppState>, id: String) -> AppResult<()> {
    instance::delete_instance(&state.paths, &id)?;
    let mut settings = state.settings.lock().clone();
    if settings.selected_instance_id.as_deref() == Some(id.as_str()) {
        settings.selected_instance_id = None;
        settings.save(&state.paths)?;
        *state.settings.lock() = settings;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_loader_versions(
    loader: ModLoader,
    game_version: String,
) -> AppResult<Vec<LoaderVersionInfo>> {
    loader::list_loader_versions(loader, &game_version).await
}

#[tauri::command]
pub async fn install_instance(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let mut inst = instance::get_instance(&state.paths, &id)?;
    let cancel = CancellationToken::new();
    {
        let mut slot = state.install_cancel.lock();
        if let Some(existing) = slot.as_ref() {
            existing.cancel();
        }
        *slot = Some(cancel.clone());
    }

    let paths = state.paths.clone();
    let result = loader::install_instance_full(&app, &paths, &mut inst, cancel).await;

    {
        let mut slot = state.install_cancel.lock();
        *slot = None;
    }

    match &result {
        Ok(()) => {
            let _ = app.emit("install-complete", serde_json::json!({ "id": id, "ok": true }));
        }
        Err(AppError::Cancelled) => {
            let _ = app.emit(
                "install-complete",
                serde_json::json!({ "id": id, "ok": false, "cancelled": true }),
            );
        }
        Err(err) => {
            let _ = app.emit(
                "install-complete",
                serde_json::json!({ "id": id, "ok": false, "error": err.to_string() }),
            );
        }
    }
    result
}

#[tauri::command]
pub fn cancel_install(state: State<'_, AppState>) -> AppResult<()> {
    if let Some(token) = state.install_cancel.lock().as_ref() {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn search_mods(
    state: State<'_, AppState>,
    instance_id: String,
    query: String,
) -> AppResult<Vec<ModHit>> {
    let inst = instance::get_instance(&state.paths, &instance_id)?;
    mods::search_mods(&query, &inst.version_id, inst.loader, 40).await
}

#[tauri::command]
pub async fn install_mod(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    project_id: String,
) -> AppResult<String> {
    let inst = instance::get_instance(&state.paths, &instance_id)?;
    let cancel = CancellationToken::new();
    {
        let mut slot = state.install_cancel.lock();
        *slot = Some(cancel.clone());
    }
    let paths = state.paths.clone();
    let result = mods::install_mod(
        &app,
        &paths,
        &instance_id,
        &project_id,
        &inst.version_id,
        inst.loader,
        cancel,
    )
    .await;
    *state.install_cancel.lock() = None;
    result
}

#[tauri::command]
pub fn list_installed_mods(
    state: State<'_, AppState>,
    instance_id: String,
) -> AppResult<Vec<String>> {
    instance::list_installed_mods(&state.paths, &instance_id)
}

#[tauri::command]
pub fn remove_mod(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> AppResult<()> {
    instance::remove_mod(&state.paths, &instance_id, &file_name)
}

#[tauri::command]
pub async fn login(app: AppHandle, state: State<'_, AppState>) -> AppResult<AccountInfo> {
    let client_id = state.settings.lock().microsoft_client_id.clone();
    let session = auth::login_with_browser(&app, &client_id).await?;
    let _ = auth::clear_offline_session(&state.paths);
    let info = auth::account_info(&session);
    *state.session.lock() = Some(session);
    Ok(info)
}

#[tauri::command]
pub fn login_offline(state: State<'_, AppState>, username: String) -> AppResult<AccountInfo> {
    let session = auth::login_offline(&state.paths, &username)?;
    let _ = auth::logout_microsoft();
    let info = auth::account_info(&session);
    *state.session.lock() = Some(session);
    Ok(info)
}

#[tauri::command]
pub async fn restore_session(state: State<'_, AppState>) -> AppResult<Option<AccountInfo>> {
    if let Some(session) = auth::load_offline_session(&state.paths)? {
        let info = auth::account_info(&session);
        *state.session.lock() = Some(session);
        return Ok(Some(info));
    }

    let client_id = state.settings.lock().microsoft_client_id.clone();
    if client_id.trim().is_empty() {
        return Ok(None);
    }
    match auth::refresh_session(&client_id).await {
        Ok(Some(session)) => {
            let info = auth::account_info(&session);
            *state.session.lock() = Some(session);
            Ok(Some(info))
        }
        Ok(None) => Ok(None),
        Err(AppError::MissingClientId) => Ok(None),
        Err(err) => {
            eprintln!("session restore failed: {err}");
            let _ = auth::logout(&state.paths);
            *state.session.lock() = None;
            Err(err)
        }
    }
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) -> AppResult<()> {
    auth::logout(&state.paths)?;
    *state.session.lock() = None;
    Ok(())
}

#[tauri::command]
pub fn get_account(state: State<'_, AppState>) -> Option<AccountInfo> {
    state
        .session
        .lock()
        .as_ref()
        .map(auth::account_info)
}

#[tauri::command]
pub fn list_java() -> AppResult<Vec<JavaRuntime>> {
    java::list_detected_java()
}

#[tauri::command]
pub async fn launch_instance(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state
        .session
        .lock()
        .clone()
        .ok_or(AppError::NotSignedIn)?;
    let settings = state.settings.lock().clone();
    let paths = state.paths.clone();

    let child = crate::launch::launch_vanilla(
        &app,
        &paths,
        &session,
        crate::launch::LaunchOptions {
            instance_id: id.clone(),
            memory_mib: settings.memory_mib,
            java_override: settings.java_path_override.map(Into::into),
        },
    )
    .await?;

    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let mut child = child;
        let status = child.wait();
        let payload = match status {
            Ok(code) => serde_json::json!({ "id": id, "ok": code.success(), "code": code.code() }),
            Err(err) => serde_json::json!({ "id": id, "ok": false, "error": err.to_string() }),
        };
        let _ = app_handle.emit("game-exited", payload);
    });

    Ok(())
}

#[tauri::command]
pub fn open_azure_setup() -> AppResult<()> {
    open::that(
        "https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/CreateApplicationBlade",
    )
    .map_err(|err| AppError::msg(format!("Failed to open Azure Portal: {err}")))?;
    Ok(())
}

#[tauri::command]
pub fn open_mojang_app_review() -> AppResult<()> {
    open::that("https://aka.ms/mce-reviewappid")
        .map_err(|err| AppError::msg(format!("Failed to open Mojang review form: {err}")))?;
    Ok(())
}

#[tauri::command]
pub fn data_paths(state: State<'_, AppState>) -> serde_json::Value {
    serde_json::json!({
        "config": state.paths.config_dir,
        "data": state.paths.data_dir,
        "cache": state.paths.cache_dir,
    })
}
