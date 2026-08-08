use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};

const AUTH_BASE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPES: &str = "XboxLive.SignIn XboxLive.offline_access";
const LOGIN_CODE_EVENT: &str = "login-code";

#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCodePayload {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    user_code: String,
    device_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
    message: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenJson {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

fn require_client_id(client_id: &str) -> AppResult<&str> {
    let id = client_id.trim();
    if id.is_empty() {
        return Err(AppError::MissingClientId);
    }
    Ok(id)
}

/// Opens the system browser and completes Microsoft sign-in via OAuth device code.
/// Never collects a Microsoft password inside the app.
///
/// `client_id` must be a Microsoft Entra app registered as **Cubic** so the
/// consent screen shows Cubic (not another launcher).
pub async fn authorize_with_browser(
    app: &AppHandle,
    client_id: &str,
) -> AppResult<TokenResponse> {
    let client_id = require_client_id(client_id)?;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{AUTH_BASE}/devicecode"))
        .form(&[("client_id", client_id), ("scope", SCOPES)])
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    let device: DeviceCodeResponse = serde_json::from_str(&body).map_err(|err| {
        AppError::OAuth(format!(
            "Invalid device-code response ({status}): {err}; body={body}"
        ))
    })?;

    if let Some(error) = device.error {
        return Err(AppError::OAuth(format!(
            "{error}: {}",
            device.error_description.unwrap_or_default()
        )));
    }

    let message = device.message.clone().unwrap_or_else(|| {
        format!(
            "Open {} and enter code {}",
            device.verification_uri, device.user_code
        )
    });

    app.emit(
        LOGIN_CODE_EVENT,
        LoginCodePayload {
            user_code: device.user_code.clone(),
            verification_uri: device.verification_uri.clone(),
            message: message.clone(),
        },
    )
    .map_err(|err| AppError::msg(format!("Failed to emit login code: {err}")))?;

    open::that(&device.verification_uri).map_err(|err| {
        AppError::OAuth(format!(
            "Failed to open system browser for Microsoft sign-in: {err}"
        ))
    })?;

    poll_for_token(
        &client,
        client_id,
        &device.device_code,
        device.interval.unwrap_or(5),
        device.expires_in,
    )
    .await
}

pub async fn refresh_access_token(
    client_id: &str,
    refresh_token: &str,
) -> AppResult<TokenResponse> {
    let client_id = require_client_id(client_id)?;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{AUTH_BASE}/token"))
        .form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", SCOPES),
        ])
        .send()
        .await?;

    parse_token_response(response).await
}

async fn poll_for_token(
    client: &reqwest::Client,
    client_id: &str,
    device_code: &str,
    interval_secs: u64,
    expires_in: u64,
) -> AppResult<TokenResponse> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(expires_in);
    let mut interval = Duration::from_secs(interval_secs.max(1));

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::OAuth(
                "Timed out waiting for Microsoft sign-in".into(),
            ));
        }

        tokio::time::sleep(interval).await;

        let response = client
            .post(format!("{AUTH_BASE}/token"))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", device_code),
            ])
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        let parsed: OAuthTokenJson = serde_json::from_str(&body).map_err(|err| {
            AppError::OAuth(format!(
                "Invalid token poll response ({status}): {err}; body={body}"
            ))
        })?;

        if let Some(error) = parsed.error.as_deref() {
            match error {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval += Duration::from_secs(5);
                    continue;
                }
                "expired_token" => {
                    return Err(AppError::OAuth(
                        "Microsoft sign-in code expired. Try again.".into(),
                    ));
                }
                "access_denied" => {
                    return Err(AppError::OAuth(
                        "Microsoft sign-in was cancelled.".into(),
                    ));
                }
                other => {
                    return Err(AppError::OAuth(format!(
                        "{other}: {}",
                        parsed.error_description.unwrap_or_default()
                    )));
                }
            }
        }

        let access_token = parsed
            .access_token
            .ok_or_else(|| AppError::OAuth("Token response missing access_token".into()))?;

        return Ok(TokenResponse {
            access_token,
            refresh_token: parsed.refresh_token,
        });
    }
}

async fn parse_token_response(response: reqwest::Response) -> AppResult<TokenResponse> {
    let status = response.status();
    let body = response.text().await?;
    let parsed: OAuthTokenJson = serde_json::from_str(&body).map_err(|err| {
        AppError::OAuth(format!(
            "Invalid token response ({status}): {err}; body={body}"
        ))
    })?;

    if let Some(error) = parsed.error {
        return Err(AppError::OAuth(format!(
            "{error}: {}",
            parsed.error_description.unwrap_or_default()
        )));
    }

    let access_token = parsed
        .access_token
        .ok_or_else(|| AppError::OAuth("Token response missing access_token".into()))?;

    Ok(TokenResponse {
        access_token,
        refresh_token: parsed.refresh_token,
    })
}
