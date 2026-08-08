use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::download::progress::DownloadProgress;
use crate::download::verify::verify_sha1;
use crate::error::{AppError, AppResult};
use crate::metadata::client::{ensure_version_json, fetch_asset_index};
use crate::metadata::types::{rules_allow, Artifact, Library, VersionJson};
use crate::paths::AppPaths;

const ASSET_BASE: &str = "https://resources.download.minecraft.net";
const PROGRESS_EVENT: &str = "download-progress";

struct PlannedFile {
    url: String,
    dest: PathBuf,
    sha1: String,
    size: u64,
    label: String,
}

pub async fn install_vanilla(
    app: &AppHandle,
    paths: &AppPaths,
    version_id: &str,
    cancel: CancellationToken,
) -> AppResult<()> {
    let version = ensure_version_json(paths, version_id).await?;
    let downloads = version.downloads.as_ref().ok_or_else(|| {
        AppError::msg(format!("Version {version_id} is missing client downloads"))
    })?;
    let mut planned = Vec::new();

    planned.push(PlannedFile {
        url: downloads.client.url.clone(),
        dest: paths.version_jar_path(version_id),
        sha1: downloads.client.sha1.clone(),
        size: downloads.client.size,
        label: format!("{version_id}.jar"),
    });

    plan_libraries(paths, &version, &mut planned)?;

    let asset_index = version
        .asset_index
        .as_ref()
        .ok_or_else(|| AppError::msg(format!("Version {version_id} is missing assetIndex")))?;

    let indexes_dir = paths.assets_dir().join("indexes");
    tokio::fs::create_dir_all(&indexes_dir).await?;
    let index_path = indexes_dir.join(format!("{}.json", asset_index.id));
    planned.push(PlannedFile {
        url: asset_index.url.clone(),
        dest: index_path.clone(),
        sha1: asset_index.sha1.clone(),
        size: asset_index.size,
        label: format!("asset-index-{}", asset_index.id),
    });

    // Download index first if needed so we can plan assets accurately.
    download_one(app, paths, &planned[planned.len() - 1], "metadata", 0, 1, &cancel).await?;
    let asset_index_data = if index_path.exists() {
        let raw = tokio::fs::read_to_string(&index_path).await?;
        serde_json::from_str(&raw)?
    } else {
        fetch_asset_index(&asset_index.url).await?
    };

    // Remove the index from planned since it is already done; rebuild list for remaining work.
    let mut remaining: Vec<PlannedFile> = planned
        .into_iter()
        .filter(|f| f.dest != index_path)
        .collect();

    let objects_dir = paths.assets_dir().join("objects");
    for (name, obj) in asset_index_data.objects {
        if obj.hash.len() < 2 {
            return Err(AppError::msg(format!(
                "Asset object '{name}' has an invalid hash"
            )));
        }
        let prefix = &obj.hash[..2];
        let dest = objects_dir.join(prefix).join(&obj.hash);
        remaining.push(PlannedFile {
            url: format!("{ASSET_BASE}/{prefix}/{}", obj.hash),
            dest,
            sha1: obj.hash,
            size: obj.size,
            label: name,
        });
    }

    let files_total = remaining.len() as u64;
    let files_done = Arc::new(AtomicU64::new(0));
    let client = reqwest::Client::new();

    // Bounded concurrency
    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
    let mut handles = Vec::new();

    for file in remaining {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let permit = semaphore.clone().acquire_owned().await.map_err(|_| {
            AppError::msg("Download semaphore closed unexpectedly")
        })?;
        let app = app.clone();
        let paths = paths.clone();
        let cancel = cancel.clone();
        let client = client.clone();
        let files_done = files_done.clone();

        handles.push(tokio::spawn(async move {
            let _permit = permit;
            download_file(
                &app,
                &client,
                &paths,
                &file,
                "install",
                files_done.load(Ordering::Relaxed),
                files_total,
                &cancel,
            )
            .await?;
            let done = files_done.fetch_add(1, Ordering::Relaxed) + 1;
            emit_progress(
                &app,
                DownloadProgress {
                    phase: "install".into(),
                    id: file.label.clone(),
                    file: file.dest.display().to_string(),
                    bytes_done: file.size,
                    bytes_total: file.size,
                    files_done: done,
                    files_total,
                },
            )?;
            Ok::<(), AppError>(())
        }));
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(err) => return Err(AppError::msg(format!("Download task failed: {err}"))),
        }
    }

    Ok(())
}

fn plan_libraries(
    paths: &AppPaths,
    version: &VersionJson,
    planned: &mut Vec<PlannedFile>,
) -> AppResult<()> {
    let features = HashMap::new();
    for lib in &version.libraries {
        if !rules_allow(lib.rules.as_deref(), &features) {
            continue;
        }
        if lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()).is_some() {
            if let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
                push_artifact(paths, artifact, planned)?;
            }
        } else {
            let (rel, url) = crate::metadata::resolve::library_url(lib)?;
            planned.push(PlannedFile {
                url,
                dest: paths.libraries_dir().join(&rel),
                sha1: String::new(),
                size: 0,
                label: rel,
            });
        }
        if let Some(natives_key) = native_classifier(lib) {
            if let Some(classifier) = lib
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.get(&natives_key))
            {
                push_artifact(paths, classifier, planned)?;
            }
        }
    }
    Ok(())
}

fn native_classifier(lib: &Library) -> Option<String> {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };
    lib.natives.as_ref()?.get(os).cloned()
}

fn push_artifact(
    paths: &AppPaths,
    artifact: &Artifact,
    planned: &mut Vec<PlannedFile>,
) -> AppResult<()> {
    let rel = artifact
        .path
        .clone()
        .ok_or_else(|| AppError::msg(format!("Library artifact missing path: {}", artifact.url)))?;
    let dest = paths.libraries_dir().join(&rel);
    planned.push(PlannedFile {
        url: artifact.url.clone(),
        dest,
        sha1: artifact.sha1.clone(),
        size: artifact.size,
        label: rel,
    });
    Ok(())
}

async fn download_one(
    app: &AppHandle,
    paths: &AppPaths,
    file: &PlannedFile,
    phase: &str,
    files_done: u64,
    files_total: u64,
    cancel: &CancellationToken,
) -> AppResult<()> {
    let client = reqwest::Client::new();
    download_file(
        app,
        &client,
        paths,
        file,
        phase,
        files_done,
        files_total,
        cancel,
    )
    .await
}

async fn download_file(
    app: &AppHandle,
    client: &reqwest::Client,
    paths: &AppPaths,
    file: &PlannedFile,
    phase: &str,
    files_done: u64,
    files_total: u64,
    cancel: &CancellationToken,
) -> AppResult<()> {
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    if file.dest.exists() {
        if file.sha1.is_empty() {
            return Ok(());
        }
        match verify_sha1(&file.dest, &file.sha1).await {
            Ok(()) => return Ok(()),
            Err(AppError::HashMismatch { .. }) => {
                tokio::fs::remove_file(&file.dest).await?;
            }
            Err(err) => return Err(err),
        }
    }

    if let Some(parent) = file.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::create_dir_all(paths.download_tmp_dir()).await?;
    let tmp = unique_tmp(paths, &file.dest)?;

    let response = client.get(&file.url).send().await?;
    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "Download failed for {}: {}",
            file.url,
            response.status()
        )));
    }

    let total = response.content_length().unwrap_or(file.size);
    let mut stream = response.bytes_stream();
    let mut out = tokio::fs::File::create(&tmp).await?;
    let mut done = 0u64;

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            drop(out);
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(AppError::Cancelled);
        }
        let chunk = chunk?;
        out.write_all(&chunk).await?;
        done += chunk.len() as u64;
        emit_progress(
            app,
            DownloadProgress {
                phase: phase.into(),
                id: file.label.clone(),
                file: file.dest.display().to_string(),
                bytes_done: done,
                bytes_total: total,
                files_done,
                files_total,
            },
        )?;
    }
    out.flush().await?;
    drop(out);

    if !file.sha1.is_empty() {
        verify_sha1(&tmp, &file.sha1).await.map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            err
        })?;
    }

    tokio::fs::rename(&tmp, &file.dest).await?;
    Ok(())
}

/// Download an arbitrary file (used for mods / loader libs).
pub async fn download_url_to(
    app: &AppHandle,
    paths: &AppPaths,
    url: &str,
    dest: &Path,
    sha1: Option<&str>,
    label: &str,
    cancel: &CancellationToken,
) -> AppResult<()> {
    let file = PlannedFile {
        url: url.to_string(),
        dest: dest.to_path_buf(),
        sha1: sha1.unwrap_or("").to_string(),
        size: 0,
        label: label.to_string(),
    };
    let client = reqwest::Client::new();
    download_file(app, &client, paths, &file, "mods", 0, 1, cancel).await
}

/// Download libraries for a (possibly inherited) version profile.
pub async fn install_version_libraries(
    app: &AppHandle,
    paths: &AppPaths,
    version: &VersionJson,
    cancel: &CancellationToken,
) -> AppResult<()> {
    let mut planned = Vec::new();
    plan_libraries(paths, version, &mut planned)?;
    let files_total = planned.len() as u64;
    let client = reqwest::Client::new();
    for (i, file) in planned.iter().enumerate() {
        download_file(
            app,
            &client,
            paths,
            file,
            "libraries",
            i as u64,
            files_total,
            cancel,
        )
        .await?;
    }
    Ok(())
}

fn unique_tmp(paths: &AppPaths, dest: &Path) -> AppResult<PathBuf> {
    let name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let id = uuid::Uuid::new_v4();
    Ok(paths
        .download_tmp_dir()
        .join(format!("{name}.{id}.part")))
}

fn emit_progress(app: &AppHandle, progress: DownloadProgress) -> AppResult<()> {
    app.emit(PROGRESS_EVENT, progress)
        .map_err(|err| AppError::msg(format!("Failed to emit progress: {err}")))
}
