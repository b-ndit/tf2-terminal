use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::services::simulator_service::{self, ItemKeyInput, SimulatedTradeView};

/// Values a hypothetical trade the user assembled by hand (Module 13's
/// drag-drop builder) — same valuation/rating engine as Module 9's real
/// trade offers, see `services::simulator_service` for how "give"
/// (owned, by asset id) vs "receive" (hypothetical, by `ItemKey`) differ.
#[tauri::command]
#[specta::specta]
pub async fn simulate_trade(
    state: State<'_, AppState>,
    given_asset_ids: Vec<String>,
    received_item_keys: Vec<ItemKeyInput>,
) -> AppResult<SimulatedTradeView> {
    let steam_id = state
        .config
        .read()
        .await
        .steam_id
        .ok_or_else(|| AppError::Config("not logged in with Steam".to_string()))?;

    simulator_service::simulate_trade(
        &state.db,
        &steam_id.to_string(),
        &given_asset_ids,
        &received_item_keys,
        now_unix(),
    )
    .await
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}
