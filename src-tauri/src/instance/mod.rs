use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ModLoader {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl ModLoader {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    pub fn supports_mods(self) -> bool {
        !matches!(self, Self::Vanilla)
    }

    pub fn is_implemented(self) -> bool {
        matches!(self, Self::Vanilla | Self::Fabric | Self::Quilt)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version_id: String,
    #[serde(default)]
    pub loader: ModLoader,
    #[serde(default)]
    pub loader_version: Option<String>,
    /// Resolved profile id used to launch (e.g. fabric-loader-…-1.21.1).
    #[serde(default)]
    pub launch_version_id: Option<String>,
    pub created_at: String,
}

impl Instance {
    pub fn effective_launch_id(&self) -> &str {
        self.launch_version_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(self.version_id.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInstanceRequest {
    pub name: String,
    pub version_id: String,
    #[serde(default)]
    pub loader: ModLoader,
    #[serde(default)]
    pub loader_version: Option<String>,
}

pub fn list_instances(paths: &AppPaths) -> AppResult<Vec<Instance>> {
    let dir = paths.instances_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let meta = entry.path().join("instance.json");
        if !meta.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(meta)?;
        let instance: Instance = serde_json::from_str(&raw)?;
        out.push(instance);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

pub fn create_instance(paths: &AppPaths, req: CreateInstanceRequest) -> AppResult<Instance> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("Instance name cannot be empty"));
    }
    if req.version_id.trim().is_empty() {
        return Err(AppError::msg("Version is required"));
    }
    if !req.loader.is_implemented() {
        return Err(AppError::msg(format!(
            "{} loader support is not ready yet — pick Vanilla, Fabric, or Quilt",
            req.loader.as_str()
        )));
    }

    let id = Uuid::new_v4().to_string();
    let instance = Instance {
        id: id.clone(),
        name,
        version_id: req.version_id,
        loader: req.loader,
        loader_version: req.loader_version,
        launch_version_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let dir = paths.instance_dir(&id);
    std::fs::create_dir_all(paths.instance_game_dir(&id))?;
    std::fs::create_dir_all(paths.instance_mods_dir(&id))?;
    let raw = serde_json::to_string_pretty(&instance)?;
    std::fs::write(dir.join("instance.json"), raw)?;
    Ok(instance)
}

pub fn get_instance(paths: &AppPaths, id: &str) -> AppResult<Instance> {
    let meta = paths.instance_dir(id).join("instance.json");
    if !meta.exists() {
        return Err(AppError::InstanceNotFound(id.to_string()));
    }
    let raw = std::fs::read_to_string(meta)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn save_instance(paths: &AppPaths, instance: &Instance) -> AppResult<()> {
    let dir = paths.instance_dir(&instance.id);
    std::fs::create_dir_all(&dir)?;
    let raw = serde_json::to_string_pretty(instance)?;
    std::fs::write(dir.join("instance.json"), raw)?;
    Ok(())
}

pub fn delete_instance(paths: &AppPaths, id: &str) -> AppResult<()> {
    let dir = paths.instance_dir(id);
    if !dir.exists() {
        return Err(AppError::InstanceNotFound(id.to_string()));
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

pub fn list_installed_mods(paths: &AppPaths, id: &str) -> AppResult<Vec<String>> {
    let dir = paths.instance_mods_dir(id);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".jar") || name.ends_with(".jar.disabled") {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

pub fn remove_mod(paths: &AppPaths, id: &str, file_name: &str) -> AppResult<()> {
    let safe = PathSanitize(file_name);
    if !safe.is_ok() {
        return Err(AppError::msg("Invalid mod file name"));
    }
    let path = paths.instance_mods_dir(id).join(file_name);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

struct PathSanitize<'a>(&'a str);
impl PathSanitize<'_> {
    fn is_ok(&self) -> bool {
        !self.0.is_empty()
            && !self.0.contains('/')
            && !self.0.contains('\\')
            && !self.0.contains("..")
    }
}
