use crate::error::{AppError, AppResult};
use crate::metadata::types::{AssetIndex, VersionJson, VersionManifest};
use crate::paths::AppPaths;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

pub async fn fetch_version_manifest() -> AppResult<VersionManifest> {
    let client = reqwest::Client::new();
    let response = client.get(MANIFEST_URL).send().await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Failed to fetch version manifest: {}",
            response.status()
        )));
    }
    Ok(response.json().await?)
}

pub async fn fetch_version_json(url: &str) -> AppResult<VersionJson> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Failed to fetch version JSON: {}",
            response.status()
        )));
    }
    Ok(response.json().await?)
}

pub async fn fetch_asset_index(url: &str) -> AppResult<AssetIndex> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Failed to fetch asset index: {}",
            response.status()
        )));
    }
    Ok(response.json().await?)
}

pub async fn ensure_version_json(
    paths: &AppPaths,
    version_id: &str,
) -> AppResult<VersionJson> {
    let local = paths.version_json_path(version_id);
    if local.exists() {
        let raw = tokio::fs::read_to_string(&local).await?;
        return Ok(serde_json::from_str(&raw)?);
    }

    let manifest = fetch_version_manifest().await?;
    let info = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| AppError::VersionNotFound(version_id.to_string()))?;

    let json = fetch_version_json(&info.url).await?;
    tokio::fs::create_dir_all(paths.version_dir(version_id)).await?;
    let pretty = serde_json::to_string_pretty(&json)?;
    tokio::fs::write(&local, pretty).await?;
    Ok(json)
}
