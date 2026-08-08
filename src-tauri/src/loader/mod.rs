use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::download::{install_vanilla, install_version_libraries};
use crate::error::{AppError, AppResult};
use crate::instance::{Instance, ModLoader};
use crate::metadata::resolve::{resolve_version, write_version_profile};
use crate::metadata::types::VersionJson;
use crate::paths::AppPaths;

const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const QUILT_META: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersionInfo {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderMeta,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderMeta {
    version: String,
    stable: bool,
}

#[derive(Debug, Deserialize)]
struct QuiltLoaderEntry {
    loader: QuiltLoaderMeta,
}

#[derive(Debug, Deserialize)]
struct QuiltLoaderMeta {
    version: String,
}

pub async fn list_loader_versions(
    loader: ModLoader,
    game_version: &str,
) -> AppResult<Vec<LoaderVersionInfo>> {
    match loader {
        ModLoader::Fabric => list_fabric(game_version).await,
        ModLoader::Quilt => list_quilt(game_version).await,
        ModLoader::Vanilla => Ok(Vec::new()),
        ModLoader::Forge | ModLoader::NeoForge => Err(AppError::msg(format!(
            "{} isn’t available in Cubic yet — use Fabric or Quilt",
            loader.as_str()
        ))),
    }
}

async fn list_fabric(game_version: &str) -> AppResult<Vec<LoaderVersionInfo>> {
    let url = format!("{FABRIC_META}/versions/loader/{game_version}");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Cubic/0.1 (github.com/twinsson/Cubic)")
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Fabric meta error {}: {}",
            response.status(),
            game_version
        )));
    }
    let entries: Vec<FabricLoaderEntry> = response.json().await?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersionInfo {
            version: e.loader.version,
            stable: e.loader.stable,
        })
        .collect())
}

async fn list_quilt(game_version: &str) -> AppResult<Vec<LoaderVersionInfo>> {
    let url = format!("{QUILT_META}/versions/loader/{game_version}");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Cubic/0.1 (github.com/twinsson/Cubic)")
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Quilt meta error {}: {}",
            response.status(),
            game_version
        )));
    }
    let entries: Vec<QuiltLoaderEntry> = response.json().await?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersionInfo {
            version: e.loader.version,
            stable: true,
        })
        .collect())
}

async fn fetch_fabric_profile(game: &str, loader: &str) -> AppResult<VersionJson> {
    let url = format!("{FABRIC_META}/versions/loader/{game}/{loader}/profile/json");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Cubic/0.1 (github.com/twinsson/Cubic)")
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Failed to fetch Fabric profile: {}",
            response.status()
        )));
    }
    Ok(response.json().await?)
}

async fn fetch_quilt_profile(game: &str, loader: &str) -> AppResult<VersionJson> {
    let url = format!("{QUILT_META}/versions/loader/{game}/{loader}/profile/json");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Cubic/0.1 (github.com/twinsson/Cubic)")
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Failed to fetch Quilt profile: {}",
            response.status()
        )));
    }
    Ok(response.json().await?)
}

pub async fn install_instance_full(
    app: &AppHandle,
    paths: &AppPaths,
    instance: &mut Instance,
    cancel: CancellationToken,
) -> AppResult<()> {
    // Always install the vanilla game version first.
    install_vanilla(app, paths, &instance.version_id, cancel.clone()).await?;

    match instance.loader {
        ModLoader::Vanilla => {
            instance.launch_version_id = Some(instance.version_id.clone());
            crate::instance::save_instance(paths, instance)?;
            Ok(())
        }
        ModLoader::Fabric => {
            let loader_ver = resolve_loader_version(ModLoader::Fabric, instance).await?;
            let profile = fetch_fabric_profile(&instance.version_id, &loader_ver).await?;
            write_version_profile(paths, &profile).await?;
            let resolved = resolve_version(paths, &profile.id).await?;
            install_version_libraries(app, paths, &resolved, &cancel).await?;
            instance.loader_version = Some(loader_ver);
            instance.launch_version_id = Some(profile.id);
            crate::instance::save_instance(paths, instance)?;
            Ok(())
        }
        ModLoader::Quilt => {
            let loader_ver = resolve_loader_version(ModLoader::Quilt, instance).await?;
            let profile = fetch_quilt_profile(&instance.version_id, &loader_ver).await?;
            write_version_profile(paths, &profile).await?;
            let resolved = resolve_version(paths, &profile.id).await?;
            install_version_libraries(app, paths, &resolved, &cancel).await?;
            instance.loader_version = Some(loader_ver);
            instance.launch_version_id = Some(profile.id);
            crate::instance::save_instance(paths, instance)?;
            Ok(())
        }
        ModLoader::Forge | ModLoader::NeoForge => Err(AppError::msg(format!(
            "{} isn’t available in Cubic yet",
            instance.loader.as_str()
        ))),
    }
}

async fn resolve_loader_version(loader: ModLoader, instance: &Instance) -> AppResult<String> {
    if let Some(v) = &instance.loader_version {
        if !v.trim().is_empty() {
            return Ok(v.clone());
        }
    }
    let versions = list_loader_versions(loader, &instance.version_id).await?;
    let stable = versions.iter().find(|v| v.stable).or(versions.first());
    stable
        .map(|v| v.version.clone())
        .ok_or_else(|| {
            AppError::msg(format!(
                "No {} loader versions found for {}",
                loader.as_str(),
                instance.version_id
            ))
        })
}
