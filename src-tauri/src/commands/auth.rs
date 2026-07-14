use tauri::State;

use crate::app::AppState;
use crate::domain::steam_id::SteamId64;
use crate::error::AppResult;
use crate::infra::steam::auth;

/// Runs the Steam OpenID login flow (system browser) and persists the
/// resulting SteamID64 to config on success.
#[tauri::command]
#[specta::specta]
pub async fn login_with_steam(state: State<'_, AppState>) -> AppResult<SteamId64> {
    let steam_id = auth::login_via_browser().await?;

    let mut config = state.config.write().await;
    config.steam_id = Some(steam_id);
    config.save(&state.paths.config_file())?;

    Ok(steam_id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_steam_id(state: State<'_, AppState>) -> AppResult<Option<SteamId64>> {
    Ok(state.config.read().await.steam_id)
}

#[tauri::command]
#[specta::specta]
pub async fn logout_steam(state: State<'_, AppState>) -> AppResult<()> {
    let mut config = state.config.write().await;
    config.steam_id = None;
    config.save(&state.paths.config_file())
}
