use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::auth::session::AuthSession;
use crate::error::{AppError, AppResult};
use crate::metadata::types::{
    rules_allow, ArgumentValue, Library, StringOrVec, VersionJson,
};
use crate::paths::AppPaths;

pub struct LaunchOptions {
    pub instance_id: String,
    pub memory_mib: u32,
    pub java_override: Option<PathBuf>,
}

pub async fn launch_vanilla(
    app: &AppHandle,
    paths: &AppPaths,
    session: &AuthSession,
    opts: LaunchOptions,
) -> AppResult<Child> {
    let instance = crate::instance::get_instance(paths, &opts.instance_id)?;
    let launch_id = instance.effective_launch_id().to_string();
    let version = crate::metadata::resolve::resolve_version(paths, &launch_id).await?;

    let required = version
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(8);
    let component = version
        .java_version
        .as_ref()
        .and_then(|j| j.component.as_deref());
    let cancel = CancellationToken::new();
    let java = crate::java::ensure_java(
        app,
        paths,
        required,
        component,
        opts.java_override.as_deref(),
        &cancel,
    )
    .await?;

    let game_dir = paths.instance_game_dir(&opts.instance_id);
    tokio::fs::create_dir_all(&game_dir).await?;
    let _ = tokio::fs::create_dir_all(paths.instance_mods_dir(&opts.instance_id)).await;
    let natives_dir = game_dir.join("natives");
    tokio::fs::create_dir_all(&natives_dir).await?;
    extract_natives(paths, &version, &natives_dir).await?;

    let classpath = build_classpath(paths, &version, &instance.version_id)?;
    let features = default_features();
    let mut jvm_args = resolve_arguments(version.arguments.as_ref().and_then(|a| a.jvm.as_ref()), &features);
    let mut game_args = if let Some(args) = version.arguments.as_ref() {
        resolve_arguments(Some(&args.game), &features)
    } else if let Some(legacy) = &version.minecraft_arguments {
        legacy
            .split_whitespace()
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };

    let assets_root = paths.assets_dir();
    let asset_index_id = version
        .assets
        .clone()
        .or_else(|| version.asset_index.as_ref().map(|a| a.id.clone()))
        .unwrap_or_else(|| instance.version_id.clone());

    let substitutions = HashMap::from([
        (
            "${auth_player_name}".to_string(),
            session.username.clone(),
        ),
        ("${version_name}".to_string(), version.id.clone()),
        (
            "${game_directory}".to_string(),
            game_dir.display().to_string(),
        ),
        (
            "${assets_root}".to_string(),
            assets_root.display().to_string(),
        ),
        ("${assets_index_name}".to_string(), asset_index_id),
        ("${auth_uuid}".to_string(), session.uuid.clone()),
        (
            "${auth_access_token}".to_string(),
            session.access_token.clone(),
        ),
        ("${clientid}".to_string(), "cubic".to_string()),
        ("${auth_xuid}".to_string(), session.xuid.clone().unwrap_or_else(|| "0".into())),
        ("${user_type}".to_string(), if session.offline { "legacy".into() } else { "msa".into() }),
        ("${version_type}".to_string(), version.version_type.clone().unwrap_or_else(|| "release".into())),
        (
            "${natives_directory}".to_string(),
            natives_dir.display().to_string(),
        ),
        ("${launcher_name}".to_string(), "Cubic".to_string()),
        ("${launcher_version}".to_string(), env!("CARGO_PKG_VERSION").to_string()),
        ("${classpath}".to_string(), classpath.clone()),
        (
            "${library_directory}".to_string(),
            paths.libraries_dir().display().to_string(),
        ),
        ("${classpath_separator}".to_string(), ":".to_string()),
    ]);

    apply_substitutions(&mut jvm_args, &substitutions);
    apply_substitutions(&mut game_args, &substitutions);

    // Ensure memory and classpath if modern args omitted classpath somehow
    if !jvm_args.iter().any(|a| a.starts_with("-Xmx")) {
        jvm_args.insert(0, format!("-Xmx{}M", opts.memory_mib));
    }
    if !jvm_args.iter().any(|a| a == "-cp" || a == "-classpath") {
        jvm_args.push("-cp".into());
        jvm_args.push(classpath);
    }

    let mut cmd = Command::new(&java.path);
    cmd.current_dir(&game_dir)
        .args(&jvm_args)
        .arg(&version.main_class)
        .args(&game_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().map_err(|err| {
        AppError::msg(format!(
            "Failed to spawn Minecraft with {}: {err}",
            java.path.display()
        ))
    })?;
    Ok(child)
}

fn default_features() -> HashMap<String, bool> {
    HashMap::from([
        ("is_demo_user".into(), false),
        ("has_custom_resolution".into(), false),
        ("has_quick_plays_support".into(), false),
        ("is_quick_play_singleplayer".into(), false),
        ("is_quick_play_multiplayer".into(), false),
        ("is_quick_play_realms".into(), false),
    ])
}

fn resolve_arguments(
    args: Option<&Vec<ArgumentValue>>,
    features: &HashMap<String, bool>,
) -> Vec<String> {
    let Some(args) = args else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for arg in args {
        match arg {
            ArgumentValue::String(s) => out.push(s.clone()),
            ArgumentValue::Object { rules, value } => {
                if rules_allow(rules.as_deref(), features) {
                    match value {
                        StringOrVec::String(s) => out.push(s.clone()),
                        StringOrVec::Vec(v) => out.extend(v.clone()),
                    }
                }
            }
        }
    }
    out
}

fn apply_substitutions(args: &mut [String], map: &HashMap<String, String>) {
    for arg in args.iter_mut() {
        let mut replaced = arg.clone();
        for (key, value) in map {
            replaced = replaced.replace(key, value);
        }
        *arg = replaced;
    }
}

fn build_classpath(
    paths: &AppPaths,
    version: &VersionJson,
    game_jar_version_id: &str,
) -> AppResult<String> {
    let features = HashMap::new();
    let mut entries = Vec::new();
    for lib in &version.libraries {
        if !rules_allow(lib.rules.as_deref(), &features) {
            continue;
        }
        if let Ok((rel, _)) = crate::metadata::resolve::library_url(lib) {
            entries.push(paths.libraries_dir().join(rel));
        }
    }
    entries.push(paths.version_jar_path(game_jar_version_id));
    let joined = entries
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    Ok(joined)
}

async fn extract_natives(
    paths: &AppPaths,
    version: &VersionJson,
    natives_dir: &Path,
) -> AppResult<()> {
    let features = HashMap::new();
    for lib in &version.libraries {
        if !rules_allow(lib.rules.as_deref(), &features) {
            continue;
        }
        let Some(key) = native_key(lib) else {
            continue;
        };
        let Some(artifact) = lib
            .downloads
            .as_ref()
            .and_then(|d| d.classifiers.as_ref())
            .and_then(|c| c.get(&key))
        else {
            continue;
        };
        let Some(rel) = &artifact.path else {
            continue;
        };
        let jar = paths.libraries_dir().join(rel);
        if jar.exists() {
            extract_jar_natives(&jar, natives_dir).await?;
        }
    }
    Ok(())
}

fn native_key(lib: &Library) -> Option<String> {
    lib.natives.as_ref()?.get("linux").cloned()
}

async fn extract_jar_natives(jar: &Path, dest: &Path) -> AppResult<()> {
    // Use system `jar` or `unzip` to avoid adding zip crate complexity for META filtering.
    // Prefer unzip which is commonly available on Linux.
    let status = Command::new("unzip")
        .arg("-o")
        .arg("-q")
        .arg(jar)
        .arg("-d")
        .arg(dest)
        .status();

    match status {
        Ok(s) if s.success() => {
            // Remove META-INF from natives dump
            let meta = dest.join("META-INF");
            if meta.exists() {
                let _ = tokio::fs::remove_dir_all(meta).await;
            }
            Ok(())
        }
        Ok(s) => Err(AppError::msg(format!(
            "unzip failed extracting natives from {}: exit {s}",
            jar.display()
        ))),
        Err(err) => Err(AppError::msg(format!(
            "unzip is required to extract natives ({err})"
        ))),
    }
}
