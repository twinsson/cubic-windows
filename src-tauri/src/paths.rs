use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::{AppError, AppResult};

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "twinsson";
const APPLICATION: &str = "minecraft-launcher";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> AppResult<Self> {
        let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).ok_or_else(|| {
            AppError::msg("Could not resolve XDG project directories for this platform")
        })?;

        let paths = Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            cache_dir: dirs.cache_dir().to_path_buf(),
        };
        paths.ensure()?;
        Ok(paths)
    }

    pub fn ensure(&self) -> AppResult<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(self.instances_dir())?;
        std::fs::create_dir_all(self.libraries_dir())?;
        std::fs::create_dir_all(self.assets_dir())?;
        std::fs::create_dir_all(self.versions_dir())?;
        std::fs::create_dir_all(self.runtime_dir())?;
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn offline_session_file(&self) -> PathBuf {
        self.config_dir.join("offline-session.json")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instances")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.data_dir.join("libraries")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.data_dir.join("versions")
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.data_dir.join("runtime")
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.instances_dir().join(id)
    }

    pub fn instance_game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("minecraft")
    }

    pub fn instance_mods_dir(&self, id: &str) -> PathBuf {
        self.instance_game_dir(id).join("mods")
    }

    pub fn version_dir(&self, version_id: &str) -> PathBuf {
        self.versions_dir().join(version_id)
    }

    pub fn version_json_path(&self, version_id: &str) -> PathBuf {
        self.version_dir(version_id)
            .join(format!("{version_id}.json"))
    }

    pub fn version_jar_path(&self, version_id: &str) -> PathBuf {
        self.version_dir(version_id)
            .join(format!("{version_id}.jar"))
    }

    pub fn download_tmp_dir(&self) -> PathBuf {
        self.cache_dir.join("downloads")
    }
}
