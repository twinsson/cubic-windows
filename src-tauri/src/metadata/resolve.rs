use crate::error::{AppError, AppResult};
use crate::metadata::client::{ensure_version_json, fetch_version_json};
use crate::metadata::types::{
    ArgumentValue, Library, VersionArguments, VersionDownloads, VersionJson,
};
use crate::paths::AppPaths;

/// Load a version JSON from disk or Mojang manifest, including Fabric/Quilt profiles
/// that are already installed under `versions/`.
pub async fn load_version_json(paths: &AppPaths, version_id: &str) -> AppResult<VersionJson> {
    let local = paths.version_json_path(version_id);
    if local.exists() {
        let raw = tokio::fs::read_to_string(&local).await?;
        return Ok(serde_json::from_str(&raw)?);
    }
    ensure_version_json(paths, version_id).await
}

/// Merge inherited profiles into a single playable VersionJson.
pub async fn resolve_version(paths: &AppPaths, version_id: &str) -> AppResult<VersionJson> {
    let mut chain = Vec::new();
    let mut current = version_id.to_string();
    for _ in 0..8 {
        let json = load_version_json(paths, &current).await?;
        let parent = json.inherits_from.clone();
        chain.push(json);
        match parent {
            Some(p) => current = p,
            None => break,
        }
    }
    chain.reverse();
    let mut merged = chain
        .first()
        .cloned()
        .ok_or_else(|| AppError::VersionNotFound(version_id.to_string()))?;

    for overlay in chain.into_iter().skip(1) {
        merged = merge_version(merged, overlay);
    }
    Ok(merged)
}

fn merge_version(base: VersionJson, overlay: VersionJson) -> VersionJson {
    let mut libraries = base.libraries;
    libraries.extend(overlay.libraries);

    let arguments = match (base.arguments, overlay.arguments) {
        (Some(mut b), Some(o)) => {
            if let Some(oj) = o.jvm {
                let mut jvm = b.jvm.unwrap_or_default();
                jvm.extend(oj);
                b.jvm = Some(jvm);
            }
            b.game.extend(o.game);
            Some(b)
        }
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (None, None) => None,
    };

    VersionJson {
        id: overlay.id,
        version_type: overlay.version_type.or(base.version_type),
        inherits_from: None,
        main_class: if overlay.main_class.is_empty() {
            base.main_class
        } else {
            overlay.main_class
        },
        minecraft_arguments: overlay.minecraft_arguments.or(base.minecraft_arguments),
        arguments,
        libraries,
        asset_index: overlay.asset_index.or(base.asset_index),
        assets: overlay.assets.or(base.assets),
        downloads: overlay.downloads.or(base.downloads),
        java_version: overlay.java_version.or(base.java_version),
    }
}

pub fn maven_path(name: &str) -> AppResult<String> {
    // group:artifact:version[:classifier]
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return Err(AppError::msg(format!("Invalid maven coordinate: {name}")));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let file = if parts.len() >= 4 {
        format!("{artifact}-{version}-{}.jar", parts[3])
    } else {
        format!("{artifact}-{version}.jar")
    };
    Ok(format!("{group}/{artifact}/{version}/{file}"))
}

pub fn library_url(lib: &Library) -> AppResult<(String, String)> {
    if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
        if let Some(path) = &artifact.path {
            let url = if artifact.url.is_empty() {
                format!("https://libraries.minecraft.net/{path}")
            } else {
                artifact.url.clone()
            };
            return Ok((path.clone(), url));
        }
    }
    let path = maven_path(&lib.name)?;
    let base = lib
        .url
        .as_deref()
        .unwrap_or("https://libraries.minecraft.net/");
    let base = if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    };
    Ok((path.clone(), format!("{base}{path}")))
}

/// Persist an already-fetched profile JSON under versions/<id>/<id>.json
pub async fn write_version_profile(paths: &AppPaths, json: &VersionJson) -> AppResult<()> {
    let id = &json.id;
    tokio::fs::create_dir_all(paths.version_dir(id)).await?;
    let pretty = serde_json::to_string_pretty(json)?;
    tokio::fs::write(paths.version_json_path(id), pretty).await?;
    Ok(())
}

pub async fn fetch_and_store_version_url(
    paths: &AppPaths,
    version_id: &str,
    url: &str,
) -> AppResult<VersionJson> {
    let json = fetch_version_json(url).await?;
    // Prefer the expected id if the JSON id differs slightly
    let mut stored = json;
    if stored.id != version_id && !version_id.is_empty() {
        stored.id = version_id.to_string();
    }
    write_version_profile(paths, &stored).await?;
    Ok(stored)
}

#[allow(dead_code)]
pub fn empty_downloads() -> VersionDownloads {
    VersionDownloads {
        client: crate::metadata::types::Artifact {
            path: None,
            sha1: String::new(),
            size: 0,
            url: String::new(),
        },
        server: None,
    }
}

#[allow(dead_code)]
pub fn empty_args() -> VersionArguments {
    VersionArguments {
        game: Vec::<ArgumentValue>::new(),
        jvm: None,
    }
}
