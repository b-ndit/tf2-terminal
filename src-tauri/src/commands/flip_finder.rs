use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::infra::db::repos::watchlist_repo::WatchlistRepo;
use crate::services::flip_finder::{self, FlipOpportunityView};
use crate::services::market_analyzer_service;

#[derive(Debug, Clone, Serialize, Type)]
pub struct WatchlistItemView {
    // Specta forbids exporting i64 to TypeScript (silent precision-loss
    // risk); item ids are always small enough to round-trip exactly
    // through f64, same convention used for timestamps elsewhere.
    pub item_id: f64,
    pub item_name: String,
    pub added_ts: f64,
}

/// Scans every active item for flip opportunities (Module 11). Pull-based
/// — the frontend polls this on an interval, same shape as Module 9's
/// `get_active_trades`.
#[tauri::command]
#[specta::specta]
pub async fn get_flip_opportunities(
    state: State<'_, AppState>,
    min_roi_pct: Option<f64>,
    min_confidence: Option<f64>,
) -> AppResult<Vec<FlipOpportunityView>> {
    let now_ts = now_unix();
    flip_finder::scan(&state.db, now_ts, min_roi_pct, min_confidence).await
}

/// Adds the item a classifieds URL resolves to to the watchlist — reuses
/// `market_analyzer_service::resolve_item_id_from_url`, the same helper
/// Module 10's alert rules use, rather than a separate item picker.
#[tauri::command]
#[specta::specta]
pub async fn add_to_watchlist(state: State<'_, AppState>, url: String) -> AppResult<()> {
    let (item_id, _name) =
        market_analyzer_service::resolve_item_id_from_url(&state.db, &url).await?;
    WatchlistRepo::add(&state.db, item_id, now_unix()).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_from_watchlist(state: State<'_, AppState>, item_id: f64) -> AppResult<()> {
    WatchlistRepo::remove(&state.db, item_id as i64).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_watchlist(state: State<'_, AppState>) -> AppResult<Vec<WatchlistItemView>> {
    let rows = WatchlistRepo::list_with_items(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(|r| WatchlistItemView {
            item_id: r.item_id as f64,
            item_name: r.item_name,
            added_ts: r.added_ts as f64,
        })
        .collect())
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}
