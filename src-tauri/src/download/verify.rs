use std::path::Path;

use sha1::{Digest, Sha1};
use tokio::io::AsyncReadExt;

use crate::error::{AppError, AppResult};

pub async fn sha1_file(path: &Path) -> AppResult<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1024 * 64];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub async fn verify_sha1(path: &Path, expected: &str) -> AppResult<()> {
    let actual = sha1_file(path).await?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(AppError::HashMismatch {
            path: path.display().to_string(),
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}
