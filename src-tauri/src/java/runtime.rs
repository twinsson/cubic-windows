use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::download::progress::DownloadProgress;
use crate::download::verify::verify_sha1;
use crate::error::{AppError, AppResult};
use crate::java::{inspect_java, JavaRuntime};
use crate::paths::AppPaths;

const JAVA_RUNTIME_ALL: &str =
    "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const PROGRESS_EVENT: &str = "download-progress";

#[derive(Debug, Deserialize)]
struct ManifestRef {
    sha1: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    manifest: ManifestRef,
}

#[derive(Debug, Deserialize)]
struct RuntimeFileDownloads {
    raw: RuntimeDownload,
}

#[derive(Debug, Deserialize)]
struct RuntimeDownload {
    sha1: String,
    url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct RuntimeFileEntry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    downloads: Option<RuntimeFileDownloads>,
    #[serde(default)]
    executable: bool,
    #[serde(default)]
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFileEntry>,
}

pub fn component_for_major(major: u32) -> &'static str {
    match major {
        0..=8 => "jre-legacy",
        9..=16 => "java-runtime-alpha",
        17 => "java-runtime-gamma",
        18..=21 => "java-runtime-delta",
        _ => "java-runtime-epsilon",
    }
}

pub fn managed_java_bin(paths: &AppPaths, component: &str) -> PathBuf {
    paths
        .runtime_dir()
        .join(component)
        .join("bin")
        .join(AppPaths::java_bin_name())
}

pub fn find_existing_component(paths: &AppPaths, component: &str) -> Option<PathBuf> {
    let mut candidates = vec![managed_java_bin(paths, component)];
    if let Ok(appdata) = std::env::var("APPDATA") {
        candidates.push(
            PathBuf::from(&appdata)
                .join("PrismLauncher")
                .join("java")
                .join(component)
                .join("bin")
                .join(AppPaths::java_bin_name()),
        );
        candidates.push(
            PathBuf::from(&appdata)
                .join(".minecraft")
                .join("runtime")
                .join(component)
                .join("windows-x64")
                .join(component)
                .join("bin")
                .join(AppPaths::java_bin_name()),
        );
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("PrismLauncher")
                .join("java")
                .join(component)
                .join("bin")
                .join(AppPaths::java_bin_name()),
        );
    }
    #[cfg(unix)]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(
            home.join(".local/share/PrismLauncher/java")
                .join(component)
                .join("bin/java"),
        );
        candidates.push(
            home.join(".minecraft/runtime")
                .join(component)
                .join("linux")
                .join(component)
                .join("bin/java"),
        );
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Resolve a game version’s Java requirement and install Mojang’s runtime if needed.
pub async fn ensure_java_for_game_version(
    app: &AppHandle,
    paths: &AppPaths,
    version_id: &str,
    cancel: &CancellationToken,
) -> AppResult<JavaRuntime> {
    let version = crate::metadata::client::ensure_version_json(paths, version_id).await?;
    let required = version
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(8);
    let component = version
        .java_version
        .as_ref()
        .and_then(|j| j.component.as_deref());
    ensure_java(app, paths, required, component, None, cancel).await
}

/// Prefer override → managed/known component JRE → system JRE → download Mojang JRE.
pub async fn ensure_java(
    app: &AppHandle,
    paths: &AppPaths,
    required_major: u32,
    component: Option<&str>,
    override_path: Option<&Path>,
    cancel: &CancellationToken,
) -> AppResult<JavaRuntime> {
    let component = component
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| component_for_major(required_major));

    if let Some(path) = override_path {
        let runtime = inspect_java(path)?;
        if runtime.major_version >= required_major {
            return Ok(runtime);
        }
        return Err(AppError::msg(format!(
            "Configured Java at {} is major {}, need {}",
            path.display(),
            runtime.major_version,
            required_major
        )));
    }

    if let Some(bin) = find_existing_component(paths, component) {
        if let Ok(rt) = inspect_java(&bin) {
            if rt.major_version >= required_major {
                return Ok(rt);
            }
        }
    }

    if let Ok(rt) = crate::java::find_system_java(required_major) {
        return Ok(rt);
    }

    install_mojang_runtime(app, paths, component, cancel).await?;
    let bin = managed_java_bin(paths, component);
    let rt = inspect_java(&bin)?;
    if rt.major_version < required_major {
        return Err(AppError::msg(format!(
            "Installed {component} is major {}, need {required_major}",
            rt.major_version
        )));
    }
    Ok(rt)
}

async fn install_mojang_runtime(
    app: &AppHandle,
    paths: &AppPaths,
    component: &str,
    cancel: &CancellationToken,
) -> AppResult<()> {
    let dest_root = paths.runtime_dir().join(component);
    let marker = dest_root.join(".cubic-complete");
    if marker.is_file() && managed_java_bin(paths, component).is_file() {
        return Ok(());
    }

    let _ = app.emit(
        PROGRESS_EVENT,
        DownloadProgress {
            phase: "java".into(),
            id: component.into(),
            file: format!("Resolving {component}"),
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
        },
    );

    let client = reqwest::Client::new();
    let all: HashMap<String, HashMap<String, Vec<RuntimeVersion>>> = client
        .get(JAVA_RUNTIME_ALL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let platform = runtime_platform_key();
    let versions = all
        .get(platform)
        .and_then(|m| m.get(component))
        .ok_or_else(|| {
            AppError::msg(format!(
                "No Mojang Java runtime '{component}' for platform '{platform}'"
            ))
        })?;
    let meta = versions
        .first()
        .ok_or_else(|| AppError::msg(format!("Empty runtime list for {component}")))?;

    let manifest: RuntimeManifest = client
        .get(&meta.manifest.url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if dest_root.exists() {
        tokio::fs::remove_dir_all(&dest_root).await.ok();
    }
    tokio::fs::create_dir_all(&dest_root).await?;

    let mut file_jobs = Vec::new();
    let mut link_jobs = Vec::new();
    for (rel, entry) in manifest.files {
        match entry.kind.as_str() {
            "directory" => {
                tokio::fs::create_dir_all(dest_root.join(&rel)).await?;
            }
            "file" => file_jobs.push((rel, entry)),
            "link" => {
                if let Some(target) = entry.target {
                    link_jobs.push((rel, target));
                }
            }
            _ => {}
        }
    }
    file_jobs.sort_by(|a, b| a.0.cmp(&b.0));
    link_jobs.sort_by(|a, b| a.0.cmp(&b.0));

    let files_total = file_jobs.len() as u64;
    let files_done = Arc::new(AtomicU64::new(0));
    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
    let mut handles = Vec::new();

    for (rel, entry) in file_jobs {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let downloads = entry.downloads.ok_or_else(|| {
            AppError::msg(format!("Runtime file {rel} is missing downloads"))
        })?;
        let dest = dest_root.join(&rel);
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::msg("Download semaphore closed"))?;
        let app = app.clone();
        let client = client.clone();
        let cancel = cancel.clone();
        let files_done = files_done.clone();
        let executable = entry.executable;
        let sha1 = downloads.raw.sha1;
        let url = downloads.raw.url;
        let size = downloads.raw.size;

        handles.push(tokio::spawn(async move {
            let _permit = permit;
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let need = if dest.is_file() {
                verify_sha1(&dest, &sha1).await.is_err()
            } else {
                true
            };
            if need {
                download_raw(&client, &url, &dest, &sha1, &cancel).await?;
            }
            if executable {
                set_executable(&dest)?;
            }
            let done = files_done.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit(
                PROGRESS_EVENT,
                DownloadProgress {
                    phase: "java".into(),
                    id: rel.clone(),
                    file: dest.display().to_string(),
                    bytes_done: size,
                    bytes_total: size,
                    files_done: done,
                    files_total,
                },
            );
            Ok::<(), AppError>(())
        }));
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(err) => return Err(AppError::msg(format!("Java download task failed: {err}"))),
        }
    }

    for (rel, target) in link_jobs {
        let link_path = dest_root.join(&rel);
        if let Some(parent) = link_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if link_path.symlink_metadata().is_ok() {
            let _ = tokio::fs::remove_file(&link_path).await;
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link_path).map_err(|err| {
                AppError::msg(format!(
                    "Failed to create symlink {} -> {}: {err}",
                    link_path.display(),
                    target
                ))
            })?;
        }
        #[cfg(not(unix))]
        {
            let _ = (rel, target);
        }
    }

    tokio::fs::write(&marker, meta.manifest.sha1.as_bytes()).await?;
    Ok(())
}

fn runtime_platform_key() -> &'static str {
    if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            "linux"
        } else if cfg!(target_arch = "x86") {
            "linux-i386"
        } else {
            "linux"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else if cfg!(target_arch = "x86") {
            "windows-x86"
        } else {
            "windows-x64"
        }
    } else {
        "linux"
    }
}

async fn download_raw(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    sha1: &str,
    cancel: &CancellationToken,
) -> AppResult<()> {
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    let tmp = dest.with_extension("cubicpart");
    if let Some(parent) = tmp.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;
    tokio::fs::write(&tmp, &bytes).await?;
    verify_sha1(&tmp, sha1).await?;
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

fn set_executable(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(path, perms)?;
    }
    let _ = path;
    Ok(())
}
