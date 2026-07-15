use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::services::trade_history_service::{self, TradeLedgerView, TradeSyncSummary};

/// Fetches newly-completed Steam trade offers and imports them into the
/// ledger — see the Module 12 note in `services::trade_history_service`
/// for how completed items get resolved (promoting Module 9's cached
/// active-offer analysis) versus falling back to unresolved placeholders.
#[tauri::command]
#[specta::specta]
pub async fn sync_completed_trades(state: State<'_, AppState>) -> AppResult<TradeSyncSummary> {
    trade_history_service::sync_completed_trades(&state).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_trades(
    state: State<'_, AppState>,
    limit: u32,
) -> AppResult<Vec<TradeLedgerView>> {
    trade_history_service::list_trades(&state.db, limit as i64).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_trade_rating(
    state: State<'_, AppState>,
    trade_offer_id: String,
    rating: Option<i32>,
) -> AppResult<()> {
    trade_history_service::set_trade_rating(&state.db, &trade_offer_id, rating).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_trade_notes(
    state: State<'_, AppState>,
    trade_offer_id: String,
    notes: Option<String>,
) -> AppResult<()> {
    trade_history_service::set_trade_notes(&state.db, &trade_offer_id, notes.as_deref()).await
}
