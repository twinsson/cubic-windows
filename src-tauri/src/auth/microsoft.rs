use serde::Deserialize;

use crate::auth::keyring_store;
use crate::auth::oauth;
use crate::auth::session::AuthSession;
use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize)]
struct XboxAuthResponse {
    #[serde(rename = "Token")]
    token: String,
}

#[derive(Debug, Deserialize)]
struct XboxDisplayClaims {
    xui: Vec<XboxUserInfo>,
}

#[derive(Debug, Deserialize)]
struct XboxUserInfo {
    uhs: String,
}

#[derive(Debug, Deserialize)]
struct XstsResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XboxDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct McLoginResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct EntitlementsResponse {
    items: Vec<EntitlementItem>,
}

#[derive(Debug, Deserialize)]
struct EntitlementItem {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
}

pub async fn login_with_browser(
    app: &tauri::AppHandle,
    client_id: &str,
) -> AppResult<AuthSession> {
    let tokens = oauth::authorize_with_browser(app, client_id).await?;
    let session = minecraft_session_from_msa(&tokens.access_token, tokens.refresh_token).await?;
    if let Some(refresh) = &session.refresh_token {
        keyring_store::store_refresh_token(refresh)?;
    }
    Ok(session)
}

pub async fn refresh_session(client_id: &str) -> AppResult<Option<AuthSession>> {
    let Some(refresh) = keyring_store::load_refresh_token()? else {
        return Ok(None);
    };
    let tokens = oauth::refresh_access_token(client_id, &refresh).await?;
    let session = minecraft_session_from_msa(
        &tokens.access_token,
        tokens.refresh_token.or(Some(refresh)),
    )
    .await?;
    if let Some(refresh) = &session.refresh_token {
        keyring_store::store_refresh_token(refresh)?;
    }
    Ok(Some(session))
}

pub fn logout() -> AppResult<()> {
    keyring_store::clear_refresh_token()
}

async fn minecraft_session_from_msa(
    msa_access_token: &str,
    refresh_token: Option<String>,
) -> AppResult<AuthSession> {
    let client = reqwest::Client::new();

    let xbox = authenticate_xbox(&client, msa_access_token).await?;
    let xsts = authorize_xsts(&client, &xbox.token).await?;
    let uhs = xsts
        .display_claims
        .xui
        .first()
        .map(|u| u.uhs.clone())
        .ok_or_else(|| AppError::Auth("Xbox XSTS response missing user hash".into()))?;

    let mc = login_minecraft(&client, &uhs, &xsts.token).await?;
    ensure_owns_game(&client, &mc.access_token).await?;
    let profile = fetch_profile(&client, &mc.access_token).await?;

    Ok(AuthSession {
        access_token: mc.access_token,
        refresh_token,
        uuid: insert_uuid_dashes(&profile.id)?,
        username: profile.name,
        xuid: None,
        offline: false,
    })
}

async fn authenticate_xbox(
    client: &reqwest::Client,
    msa_access_token: &str,
) -> AppResult<XboxAuthResponse> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={msa_access_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let response = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_else(|_| String::new());
        return Err(AppError::Auth(format!(
            "Xbox Live authenticate failed ({status}): {text}"
        )));
    }

    Ok(response.json().await?)
}

async fn authorize_xsts(client: &reqwest::Client, xbox_token: &str) -> AppResult<XstsResponse> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbox_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let response = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => String::new(),
        };
        return Err(AppError::Auth(format!(
            "Xbox XSTS authorize failed ({status}): {text}"
        )));
    }

    Ok(response.json().await?)
}

async fn login_minecraft(
    client: &reqwest::Client,
    uhs: &str,
    xsts_token: &str,
) -> AppResult<McLoginResponse> {
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
    });

    let response = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => String::new(),
        };
        return Err(AppError::Auth(format!(
            "Minecraft login failed ({status}): {text}"
        )));
    }

    Ok(response.json().await?)
}

async fn ensure_owns_game(client: &reqwest::Client, mc_access_token: &str) -> AppResult<()> {
    let response = client
        .get("https://api.minecraftservices.com/entitlements/mcstore")
        .bearer_auth(mc_access_token)
        .send()
        .await?;

    if response.status().as_u16() == 401 || response.status().as_u16() == 404 {
        return Err(AppError::GameNotOwned);
    }

    if !response.status().is_success() {
        let status = response.status();
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => String::new(),
        };
        return Err(AppError::Auth(format!(
            "Entitlements check failed ({status}): {text}"
        )));
    }

    let entitlements: EntitlementsResponse = response.json().await?;
    let owns = entitlements.items.iter().any(|item| {
        matches!(
            item.name.as_deref(),
            Some("product_minecraft") | Some("game_minecraft")
        )
    });

    if !owns {
        return Err(AppError::GameNotOwned);
    }
    Ok(())
}

async fn fetch_profile(
    client: &reqwest::Client,
    mc_access_token: &str,
) -> AppResult<McProfileResponse> {
    let response = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_access_token)
        .send()
        .await?;

    if response.status().as_u16() == 404 {
        return Err(AppError::GameNotOwned);
    }

    if !response.status().is_success() {
        let status = response.status();
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => String::new(),
        };
        return Err(AppError::Auth(format!(
            "Profile fetch failed ({status}): {text}"
        )));
    }

    Ok(response.json().await?)
}

fn insert_uuid_dashes(id: &str) -> AppResult<String> {
    let compact: String = id.chars().filter(|c| *c != '-').collect();
    if compact.len() != 32 {
        return Err(AppError::Auth(format!("Unexpected profile UUID: {id}")));
    }
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &compact[0..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20],
        &compact[20..32]
    ))
}
