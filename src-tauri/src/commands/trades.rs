use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
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
