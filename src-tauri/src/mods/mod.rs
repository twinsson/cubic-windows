use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::download::download_url_to;
use crate::error::{AppError, AppResult};
use crate::instance::ModLoader;
use crate::paths::AppPaths;

const MODRINTH: &str = "https://api.modrinth.com/v2";
const UA: &str = "Cubic/0.1 (github.com/twinsson/Cubic)";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModVersionFile {
    pub version_id: String,
    pub name: String,
    pub version_number: String,
    pub filename: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[derive(Debug, Deserialize)]
struct SearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    icon_url: Option<String>,
    downloads: u64,
    categories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MrVersion {
    id: String,
    name: String,
    version_number: String,
    files: Vec<MrFile>,
}

#[derive(Debug, Deserialize)]
struct MrFile {
    url: String,
    filename: String,
    size: u64,
    primary: bool,
    hashes: MrHashes,
}

#[derive(Debug, Deserialize)]
struct MrHashes {
    sha1: Option<String>,
}

pub async fn search_mods(
    query: &str,
    game_version: &str,
    loader: ModLoader,
    limit: u32,
) -> AppResult<Vec<ModHit>> {
    let loader_facet = match loader {
        ModLoader::Fabric => "fabric",
        ModLoader::Quilt => "quilt",
        ModLoader::Forge => "forge",
        ModLoader::NeoForge => "neoforge",
        ModLoader::Vanilla => {
            return Err(AppError::msg(
                "Pick Fabric or Quilt on the instance before installing mods",
            ))
        }
    };

    let q = query.trim();
    // Empty query → Modrinth-style browse: most downloaded for this version/loader.
    let index = if q.is_empty() { "downloads" } else { "relevance" };

    let facets = serde_json::json!([
        ["project_type:mod"],
        [format!("versions:{game_version}")],
        [format!("categories:{loader_facet}")]
    ]);

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{MODRINTH}/search"))
        .header("User-Agent", UA)
        .query(&[
            ("query", q),
            ("limit", &limit.min(40).max(1).to_string()),
            ("index", index),
            ("facets", &facets.to_string()),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Modrinth search failed: {}",
            response.status()
        )));
    }

    let parsed: SearchResponse = response.json().await?;
    Ok(parsed
        .hits
        .into_iter()
        .map(|h| ModHit {
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            description: h.description,
            icon_url: h.icon_url,
            downloads: h.downloads,
            categories: h.categories,
        })
        .collect())
}

pub async fn latest_mod_file(
    project_id: &str,
    game_version: &str,
    loader: ModLoader,
) -> AppResult<ModVersionFile> {
    let loader_facet = match loader {
        ModLoader::Fabric => "fabric",
        ModLoader::Quilt => "quilt",
        ModLoader::Forge => "forge",
        ModLoader::NeoForge => "neoforge",
        ModLoader::Vanilla => {
            return Err(AppError::msg("Vanilla instances cannot install mods"))
        }
    };

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{MODRINTH}/project/{project_id}/version"))
        .header("User-Agent", UA)
        .query(&[
            (
                "game_versions",
                serde_json::json!([game_version]).to_string(),
            ),
            (
                "loaders",
                serde_json::json!([loader_facet]).to_string(),
            ),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Modrinth versions failed: {}",
            response.status()
        )));
    }

    let versions: Vec<MrVersion> = response.json().await?;
    let version = versions
        .first()
        .ok_or_else(|| AppError::msg("No compatible Modrinth version found"))?;
    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| AppError::msg("Modrinth version has no files"))?;

    Ok(ModVersionFile {
        version_id: version.id.clone(),
        name: version.name.clone(),
        version_number: version.version_number.clone(),
        filename: file.filename.clone(),
        url: file.url.clone(),
        sha1: file.hashes.sha1.clone(),
        size: file.size,
    })
}

pub async fn install_mod(
    app: &AppHandle,
    paths: &AppPaths,
    instance_id: &str,
    project_id: &str,
    game_version: &str,
    loader: ModLoader,
    cancel: CancellationToken,
) -> AppResult<String> {
    let file = latest_mod_file(project_id, game_version, loader).await?;
    let mods_dir = paths.instance_mods_dir(instance_id);
    tokio::fs::create_dir_all(&mods_dir).await?;
    let dest = mods_dir.join(&file.filename);
    download_url_to(
        app,
        paths,
        &file.url,
        &dest,
        file.sha1.as_deref(),
        &file.filename,
        &cancel,
    )
    .await?;
    Ok(file.filename)
}
