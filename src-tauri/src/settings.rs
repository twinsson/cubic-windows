use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Microsoft Entra Application (client) ID for an app named "Cubic".
    /// This is what Microsoft shows on the sign-in consent screen.
    pub microsoft_client_id: String,
    pub selected_instance_id: Option<String>,
    /// Max heap in MiB passed as -Xmx.
    pub memory_mib: u32,
    pub java_path_override: Option<String>,
    /// UI theme id: grass | deepslate | nether | copper | pale
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            microsoft_client_id: String::new(),
            selected_instance_id: None,
            memory_mib: 2048,
            java_path_override: None,
            theme: "grass".into(),
        }
    }
}

impl Settings {
    pub fn load(paths: &AppPaths) -> AppResult<Self> {
        let path = paths.settings_file();
        if !path.exists() {
            let settings = Self::default();
            settings.save(paths)?;
            return Ok(settings);
        }
        let raw = std::fs::read_to_string(&path)?;
        let settings = serde_json::from_str(&raw)?;
        Ok(settings)
    }

    pub fn save(&self, paths: &AppPaths) -> AppResult<()> {
        let path = paths.settings_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}
