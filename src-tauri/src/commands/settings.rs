use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::infra::config::Config;
use crate::infra::keychain::{keys, Keychain};

#[derive(Debug, Serialize, Type)]
pub struct HealthStatus {
    pub db_connected: bool,
    pub config_dir: String,
    pub data_dir: String,
}

#[tauri::command]
#[specta::specta]
pub async fn health_check(state: State<'_, AppState>) -> AppResult<HealthStatus> {
    let db_connected = sqlx::query("SELECT 1").fetch_one(&state.db).await.is_ok();
    Ok(HealthStatus {
        db_connected,
        config_dir: state.paths.config_dir.display().to_string(),
        data_dir: state.paths.data_dir.display().to_string(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_config(state: State<'_, AppState>) -> AppResult<Config> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
#[specta::specta]
pub fn set_steam_api_key(key: String) -> AppResult<()> {
    Keychain::set(keys::STEAM_API_KEY, &key)
}

#[tauri::command]
#[specta::specta]
pub fn has_steam_api_key() -> AppResult<bool> {
    Ok(Keychain::get(keys::STEAM_API_KEY)?.is_some())
}

#[tauri::command]
#[specta::specta]
pub fn clear_steam_api_key() -> AppResult<()> {
    Keychain::delete(keys::STEAM_API_KEY)
}

#[tauri::command]
#[specta::specta]
pub fn set_backpack_tf_token(token: String) -> AppResult<()> {
    Keychain::set(keys::BACKPACK_TF_TOKEN, &token)
}

#[tauri::command]
#[specta::specta]
pub fn has_backpack_tf_token() -> AppResult<bool> {
    Ok(Keychain::get(keys::BACKPACK_TF_TOKEN)?.is_some())
}

#[tauri::command]
#[specta::specta]
pub fn clear_backpack_tf_token() -> AppResult<()> {
    Keychain::delete(keys::BACKPACK_TF_TOKEN)
}

#[tauri::command]
#[specta::specta]
pub fn set_discord_webhook_url(url: String) -> AppResult<()> {
    Keychain::set(keys::DISCORD_WEBHOOK_URL, &url)
}

#[tauri::command]
#[specta::specta]
pub fn has_discord_webhook_url() -> AppResult<bool> {
    Ok(Keychain::get(keys::DISCORD_WEBHOOK_URL)?.is_some())
}

#[tauri::command]
#[specta::specta]
pub fn clear_discord_webhook_url() -> AppResult<()> {
    Keychain::delete(keys::DISCORD_WEBHOOK_URL)
}

/// Both cookies are required together — a `sessionid` without a matching
/// `steamLoginSecure` (or vice versa) can't authenticate anything, so
/// there's no useful "half set" state to support here.
#[tauri::command]
#[specta::specta]
pub fn set_steam_session(session_id: String, login_secure: String) -> AppResult<()> {
    Keychain::set(keys::STEAM_SESSION_ID, &session_id)?;
    Keychain::set(keys::STEAM_LOGIN_SECURE, &login_secure)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn has_steam_session() -> AppResult<bool> {
    Ok(Keychain::get(keys::STEAM_SESSION_ID)?.is_some()
        && Keychain::get(keys::STEAM_LOGIN_SECURE)?.is_some())
}

#[tauri::command]
#[specta::specta]
pub fn clear_steam_session() -> AppResult<()> {
    Keychain::delete(keys::STEAM_SESSION_ID)?;
    Keychain::delete(keys::STEAM_LOGIN_SECURE)?;
    Ok(())
}
