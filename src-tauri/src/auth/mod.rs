pub mod keyring_store;
pub mod microsoft;
pub mod oauth;
pub mod offline;
pub mod session;

pub use microsoft::{login_with_browser, logout as logout_microsoft, refresh_session};
pub use offline::{clear_offline_session, load_offline_session, login_offline};
pub use session::{AccountInfo, AuthSession};

pub fn account_info(session: &AuthSession) -> AccountInfo {
    AccountInfo {
        uuid: session.uuid.clone(),
        username: session.username.clone(),
        offline: session.offline,
    }
}

pub fn logout(paths: &crate::paths::AppPaths) -> crate::error::AppResult<()> {
    let _ = logout_microsoft();
    clear_offline_session(paths)
}
