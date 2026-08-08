use keyring::Entry;

use crate::error::{AppError, AppResult};

const SERVICE: &str = "com.twinsson.cubic";
const USER: &str = "microsoft-refresh-token";

pub fn store_refresh_token(token: &str) -> AppResult<()> {
    let entry = Entry::new(SERVICE, USER)?;
    entry.set_password(token)?;
    Ok(())
}

pub fn load_refresh_token() -> AppResult<Option<String>> {
    let entry = Entry::new(SERVICE, USER)?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(AppError::from(err)),
    }
}

pub fn clear_refresh_token() -> AppResult<()> {
    let entry = Entry::new(SERVICE, USER)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(AppError::from(err)),
    }
}
