//! Microsoft -> Xbox Live -> XSTS -> Minecraft Services sign-in (OAuth device-code flow).
//!
//! Needs an Azure "public client" app registration (client ID) that Mojang has approved
//! for the Minecraft API; Pristine never ships anyone else's ID.

use crate::accounts::now;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

fn five() -> u64 { 5 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "five")]
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone)]
pub struct McSession {
    pub access_token: String,
    pub expires_at: u64,
    pub uuid: Uuid,
    pub username: String,
}

fn oauth_error(v: &Value) -> String {
    format!(
        "{}: {}",
        v["error"].as_str().unwrap_or("error"),
        v["error_description"].as_str().unwrap_or("").lines().next().unwrap_or("")
    )
}

pub async fn request_device_code(client: &reqwest::Client, client_id: &str) -> Result<DeviceCode> {
    let resp = client.post(DEVICE_CODE_URL).form(&[("client_id", client_id), ("scope", SCOPE)]).send().await?;
    let ok = resp.status().is_success();
    let body: Value = resp.json().await?;
    if !ok { return Err(Error::msg(format!("Microsoft sign-in failed ({})", oauth_error(&body)))); }
    Ok(serde_json::from_value(body)?)
}

fn tokens_from(body: &Value) -> Option<MsTokens> {
    Some(MsTokens {
        access_token: body["access_token"].as_str()?.to_string(),
        refresh_token: body["refresh_token"].as_str().unwrap_or_default().to_string(),
    })
}

/// Wait until the user approves the code in their browser (or cancels / it expires).
pub async fn poll_for_token(
    client: &reqwest::Client,
    client_id: &str,
    dc: &DeviceCode,
    cancel: &AtomicBool,
) -> Result<MsTokens> {
    let deadline = Instant::now() + Duration::from_secs(dc.expires_in);
    let mut interval = dc.interval.max(1);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if cancel.load(Ordering::Relaxed) { return Err(Error::msg("Sign-in cancelled")); }
        if Instant::now() > deadline { return Err(Error::msg("The sign-in code expired. Please try again.")); }
        let body: Value = client.post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", dc.device_code.as_str()),
            ])
            .send().await?.json().await?;
        if let Some(t) = tokens_from(&body) { return Ok(t); }
        match body["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("authorization_declined") => return Err(Error::msg("Sign-in was declined")),
            Some("expired_token") => return Err(Error::msg("The sign-in code expired. Please try again.")),
            _ => return Err(Error::msg(format!("Microsoft sign-in failed ({})", oauth_error(&body)))),
        }
    }
}

pub async fn refresh_tokens(client: &reqwest::Client, client_id: &str, refresh_token: &str) -> Result<MsTokens> {
    let body: Value = client.post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send().await?.json().await?;
    match tokens_from(&body) {
        Some(mut t) => {
            // Microsoft rotates refresh tokens, but keep the old one if none is returned.
            if t.refresh_token.is_empty() { t.refresh_token = refresh_token.to_string(); }
            Ok(t)
        }
        None if body["error"] == "invalid_grant" => {
            Err(Error::msg("Your Microsoft session expired. Please sign in again."))
        }
        None => Err(Error::msg(format!("Couldn't refresh Microsoft sign-in ({})", oauth_error(&body)))),
    }
}

async fn post_json(client: &reqwest::Client, url: &str, body: Value) -> Result<(reqwest::StatusCode, Value)> {
    let r = client.post(url).header("Accept", "application/json").json(&body).send().await?;
    let status = r.status();
    Ok((status, r.json().await.unwrap_or(Value::Null)))
}

/// Human-readable explanation for XSTS error codes.
pub fn xsts_error_message(code: Option<u64>) -> String {
    match code {
        Some(2148916233) => "This Microsoft account has no Xbox profile. Sign in at minecraft.net or xbox.com once to create one.".into(),
        Some(2148916235) => "Xbox Live isn't available in your country or region.".into(),
        Some(2148916236) | Some(2148916237) => "This account needs adult verification on Xbox before it can sign in.".into(),
        Some(2148916238) => "This is a child account. An adult must add it to a Microsoft family before it can play.".into(),
        Some(c) => format!("Xbox Live sign-in was refused (error {c})."),
        None => "Xbox Live sign-in was refused.".into(),
    }
}

/// Exchange a Microsoft access token for a Minecraft session and profile.
pub async fn minecraft_session(client: &reqwest::Client, ms_access_token: &str) -> Result<McSession> {
    // 1. Xbox Live
    let (s, v) = post_json(client, XBL_URL, json!({
        "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access_token}") },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    })).await?;
    if !s.is_success() { return Err(Error::msg(format!("Xbox Live sign-in failed (HTTP {s})"))); }
    let xbl_token = v["Token"].as_str().ok_or_else(|| Error::msg("Xbox Live returned no token"))?.to_string();

    // 2. XSTS
    let (s, v) = post_json(client, XSTS_URL, json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    })).await?;
    if !s.is_success() { return Err(Error::msg(xsts_error_message(v["XErr"].as_u64()))); }
    let xsts_token = v["Token"].as_str().ok_or_else(|| Error::msg("XSTS returned no token"))?;
    let uhs = v["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or_else(|| Error::msg("XSTS returned no user hash"))?;

    // 3. Minecraft Services
    let (s, v) = post_json(client, MC_LOGIN_URL, json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") })).await?;
    if s == reqwest::StatusCode::FORBIDDEN {
        return Err(Error::msg("Minecraft rejected this app registration (HTTP 403). The Azure app must be approved for the Minecraft API."));
    }
    if !s.is_success() { return Err(Error::msg(format!("Minecraft sign-in failed (HTTP {s})"))); }
    let mc_token = v["access_token"].as_str().ok_or_else(|| Error::msg("Minecraft returned no token"))?.to_string();
    let expires_in = v["expires_in"].as_u64().unwrap_or(86_400);

    // 4. Profile (404 = account doesn't own Java Edition)
    let r = client.get(MC_PROFILE_URL).bearer_auth(&mc_token).send().await?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::msg("This Microsoft account doesn't own Minecraft: Java Edition (or has no profile yet)."));
    }
    let p: Value = r.error_for_status()?.json().await?;
    let uuid = p["id"].as_str().and_then(|s| Uuid::parse_str(s).ok()).ok_or_else(|| Error::msg("Minecraft returned an invalid profile"))?;
    let username = p["name"].as_str().ok_or_else(|| Error::msg("Minecraft returned no username"))?.to_string();

    Ok(McSession { access_token: mc_token, expires_at: now() + expires_in, uuid, username })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_code_with_default_interval() {
        let dc: DeviceCode = serde_json::from_value(json!({
            "user_code": "ABCD1234", "device_code": "xyz", "verification_uri": "https://www.microsoft.com/link", "expires_in": 900
        })).unwrap();
        assert_eq!(dc.interval, 5);
        assert_eq!(dc.user_code, "ABCD1234");
    }

    #[test]
    fn xsts_errors_are_explained() {
        assert!(xsts_error_message(Some(2148916233)).contains("no Xbox profile"));
        assert!(xsts_error_message(Some(2148916238)).contains("child account"));
        assert!(xsts_error_message(Some(1)).contains("error 1"));
        assert!(!xsts_error_message(None).is_empty());
    }

    #[test]
    fn token_extraction() {
        let t = tokens_from(&json!({"access_token": "a", "refresh_token": "r"})).unwrap();
        assert_eq!((t.access_token.as_str(), t.refresh_token.as_str()), ("a", "r"));
        assert!(tokens_from(&json!({"error": "authorization_pending"})).is_none());
    }
}
