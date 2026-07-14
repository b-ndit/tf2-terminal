use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::infra::backpack_tf::models::ListingEvent;
use crate::infra::keychain::{keys, Keychain};
use crate::services::market_analyzer_service::{self, ItemAnalytics, PriceBar};
use crate::services::market_data_service::PriceCatalogSyncSummary;

#[tauri::command]
#[specta::specta]
pub async fn sync_price_catalog(state: State<'_, AppState>) -> AppResult<PriceCatalogSyncSummary> {
    let api_key = Keychain::get(keys::BACKPACK_TF_TOKEN)?
        .ok_or_else(|| AppError::Config("backpack.tf token not set".to_string()))?;
    state
        .market_data
        .sync_price_catalog(&state.db, &api_key)
        .await
}

/// Recent listing events observed via the live websocket feed (bounded
/// ring buffer) — a basic window into `MarketDataService` until the Live
/// Feed panel (Module 10) builds a proper UI around the same stream.
#[tauri::command]
#[specta::specta]
pub async fn get_recent_listings(state: State<'_, AppState>) -> AppResult<Vec<ListingEvent>> {
    Ok(state.market_data.recent_events().await)
}

/// Parses a backpack.tf classifieds URL, resolves the named item against
/// our schema-seeded `items` table, and computes current-moment analytics
/// (spread, liquidity, demand, estimated sale/quicksell, buyers/sellers
/// tables) from our own accumulated `market_listings`.
#[tauri::command]
#[specta::specta]
pub async fn analyze_classified_url(
    state: State<'_, AppState>,
    url: String,
) -> AppResult<ItemAnalytics> {
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64;
    market_analyzer_service::analyze_classified_url(&state.db, &url, now_ts).await
}

/// Daily OHLC bars for `url`'s resolved item (`price_daily`, Module 8's
/// History Recorder), for the Market Analyzer's price chart panel.
#[tauri::command]
#[specta::specta]
pub async fn get_price_history(
    state: State<'_, AppState>,
    url: String,
) -> AppResult<Vec<PriceBar>> {
    market_analyzer_service::get_price_history(&state.db, &url).await
}
