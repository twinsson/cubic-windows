use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::auth::session::AuthSession;
use crate::paths::AppPaths;
use crate::settings::Settings;

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Mutex<Settings>,
    pub session: Mutex<Option<AuthSession>>,
    pub install_cancel: Mutex<Option<CancellationToken>>,
}

impl AppState {
    pub fn new(paths: AppPaths, settings: Settings) -> Self {
        Self {
            paths,
            settings: Mutex::new(settings),
            session: Mutex::new(None),
            install_cancel: Mutex::new(None),
        }
    }
}
