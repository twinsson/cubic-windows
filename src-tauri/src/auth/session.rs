use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub uuid: String,
    pub username: String,
    pub xuid: Option<String>,
    #[serde(default)]
    pub offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub uuid: String,
    pub username: String,
    #[serde(default)]
    pub offline: bool,
}
