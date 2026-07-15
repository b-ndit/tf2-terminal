use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::services::portfolio_service::{
    self, ItemMoverView, PlWindowsView, PortfolioSnapshotView,
};

const DAY_SECONDS: i64 = 86_400;

async fn logged_in_steam_id(state: &AppState) -> AppResult<String> {
    let steam_id = state
        .config
        .read()
        .await
        .steam_id
        .ok_or_else(|| AppError::Config("not logged in with Steam".to_string()))?;
    Ok(steam_id.to_string())
}

/// Values the whole current inventory and persists + returns a fresh
/// snapshot — the "on-demand" half of Module 12's "daily and on-demand
/// valuation snapshots" (§6); the daily half is
/// `portfolio_service::spawn_periodic_snapshot`, spawned in `app::build()`.
#[tauri::command]
#[specta::specta]
pub async fn get_portfolio_snapshot(
    state: State<'_, AppState>,
) -> AppResult<PortfolioSnapshotView> {
    let steam_id = logged_in_steam_id(&state).await?;
    portfolio_service::snapshot_now(&state.db, &steam_id, now_unix()).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_portfolio_history(
    state: State<'_, AppState>,
    days: u32,
) -> AppResult<Vec<PortfolioSnapshotView>> {
    let steam_id = logged_in_steam_id(&state).await?;
    let since_ts = now_unix() - days as i64 * DAY_SECONDS;
    portfolio_service::get_portfolio_history(&state.db, &steam_id, since_ts).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_pl_windows(state: State<'_, AppState>) -> AppResult<PlWindowsView> {
    let steam_id = logged_in_steam_id(&state).await?;
    portfolio_service::get_pl_windows(&state.db, &steam_id, now_unix()).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_winners_losers(
    state: State<'_, AppState>,
    window_days: u32,
) -> AppResult<Vec<ItemMoverView>> {
    let steam_id = logged_in_steam_id(&state).await?;
    portfolio_service::get_winners_losers(&state.db, &steam_id, window_days as i64, now_unix())
        .await
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}
