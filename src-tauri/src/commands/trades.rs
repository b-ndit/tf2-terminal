use tauri::State;

use crate::app::AppState;
use crate::domain::steam_id::SteamId64;
use crate::error::{AppError, AppResult};
use crate::infra::keychain::{keys, Keychain};
use crate::services::partner_inventory_service::{self, PartnerItemView};
use crate::services::trade_analysis_engine::{self, AnalyzedTradeOffer};

/// Fetches every active Steam trade offer the user has received and rates
/// each one (spread/liquidity/demand-based ★ rating, explanation,
/// counteroffer suggestion). Pull-based — the frontend polls this on an
/// interval rather than the backend pushing events (`docs/DESIGN.md`
/// Module 9 note in `services::trade_analysis_engine`).
#[tauri::command]
#[specta::specta]
pub async fn get_active_trades(state: State<'_, AppState>) -> AppResult<Vec<AnalyzedTradeOffer>> {
    trade_analysis_engine::get_active_trades(&state).await
}

/// Both cookies are required to call any of `infra::steam::trade_send`'s
/// endpoints — see `commands::settings::set_steam_session`.
fn steam_session_cookies() -> AppResult<(String, String)> {
    let session_id = Keychain::get(keys::STEAM_SESSION_ID)?.ok_or_else(|| {
        AppError::Config("Steam session not connected — add it in Settings".to_string())
    })?;
    let login_secure = Keychain::get(keys::STEAM_LOGIN_SECURE)?.ok_or_else(|| {
        AppError::Config("Steam session not connected — add it in Settings".to_string())
    })?;
    Ok((session_id, login_secure))
}

fn parse_trade_offer_id(raw: &str) -> AppResult<u64> {
    raw.parse()
        .map_err(|_| AppError::InvalidInput(format!("'{raw}' is not a valid trade offer id")))
}

/// Sends a brand-new trade offer to `partner_steam_id`, giving
/// `my_asset_ids` and asking for `their_asset_ids` (both plain Steam asset
/// id strings). Requires a connected Steam session (Settings) — this is
/// Steam's unofficial `tradeoffer/new/send` endpoint, not the read-only
/// `IEconService` Web API `get_active_trades` uses.
#[tauri::command]
#[specta::specta]
pub async fn send_trade_offer(
    state: State<'_, AppState>,
    partner_steam_id: String,
    my_asset_ids: Vec<String>,
    their_asset_ids: Vec<String>,
    message: String,
) -> AppResult<String> {
    let (session_id, login_secure) = steam_session_cookies()?;
    let partner = SteamId64::parse_decimal(&partner_steam_id)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;

    let trade_offer_id = state
        .steam_session
        .send_offer(
            &session_id,
            &login_secure,
            partner,
            &my_asset_ids,
            &their_asset_ids,
            &message,
        )
        .await?;

    Ok(trade_offer_id.to_string())
}

/// Accepts an already-active offer the user received. Same unofficial
/// session-cookie auth as `send_trade_offer` — Steam's official
/// `IEconService` has no accept endpoint at all.
#[tauri::command]
#[specta::specta]
pub async fn accept_trade_offer(
    state: State<'_, AppState>,
    trade_offer_id: String,
    partner_steam_id: String,
) -> AppResult<()> {
    let (session_id, login_secure) = steam_session_cookies()?;
    let partner = SteamId64::parse_decimal(&partner_steam_id)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let trade_offer_id = parse_trade_offer_id(&trade_offer_id)?;

    state
        .steam_session
        .accept_offer(&session_id, &login_secure, trade_offer_id, partner)
        .await
}

/// Declines an already-active offer the user received.
#[tauri::command]
#[specta::specta]
pub async fn decline_trade_offer(state: State<'_, AppState>, trade_offer_id: String) -> AppResult<()> {
    let (session_id, login_secure) = steam_session_cookies()?;
    let trade_offer_id = parse_trade_offer_id(&trade_offer_id)?;

    state
        .steam_session
        .decline_offer(&session_id, &login_secure, trade_offer_id)
        .await
}

/// Read-only: another Steam account's public TF2 inventory, for the
/// "propose a trade" item picker (needs only the Steam API key, same as
/// any other `GetPlayerItems` call — no session cookies involved).
#[tauri::command]
#[specta::specta]
pub async fn get_public_inventory(
    state: State<'_, AppState>,
    partner_steam_id: String,
) -> AppResult<Vec<PartnerItemView>> {
    let api_key = Keychain::get(keys::STEAM_API_KEY)?
        .ok_or_else(|| AppError::Config("Steam API key not set".to_string()))?;
    let partner = SteamId64::parse_decimal(&partner_steam_id)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;

    partner_inventory_service::get_public_inventory(&state.db, &state.steam_api, api_key, partner)
        .await
}
