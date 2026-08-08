use md5::{Digest, Md5};
use uuid::Uuid;

use crate::auth::session::AuthSession;
use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;

/// Mojang-compatible offline UUID (Java `UUID.nameUUIDFromBytes("OfflinePlayer:"+name)`).
pub fn offline_uuid(username: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // IETF variant
    Uuid::from_bytes(bytes).hyphenated().to_string()
}

pub fn validate_username(username: &str) -> AppResult<&str> {
    let name = username.trim();
    if name.is_empty() || name.len() > 16 {
        return Err(AppError::msg("Username must be 1–16 characters"));
    }
    if name.chars().any(|c| c.is_whitespace() || c == '\0') {
        return Err(AppError::msg("Username cannot contain spaces"));
    }
    Ok(name)
}

pub fn login_offline(paths: &AppPaths, username: &str) -> AppResult<AuthSession> {
    let name = validate_username(username)?;
    let session = AuthSession {
        access_token: "0".into(),
        refresh_token: None,
        uuid: offline_uuid(name),
        username: name.to_string(),
        xuid: None,
        offline: true,
    };
    save_offline_session(paths, &session)?;
    Ok(session)
}

pub fn save_offline_session(paths: &AppPaths, session: &AuthSession) -> AppResult<()> {
    let path = paths.offline_session_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(session)?;
    std::fs::write(path, data)?;
    Ok(())
}

pub fn load_offline_session(paths: &AppPaths) -> AppResult<Option<AuthSession>> {
    let path = paths.offline_session_file();
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(path)?;
    let mut session: AuthSession = serde_json::from_slice(&data)?;
    session.offline = true;
    if session.access_token.is_empty() {
        session.access_token = "0".into();
    }
    Ok(Some(session))
}

pub fn clear_offline_session(paths: &AppPaths) -> AppResult<()> {
    let path = paths.offline_session_file();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
