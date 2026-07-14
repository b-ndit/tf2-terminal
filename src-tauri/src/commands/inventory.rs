use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::services::backpack_service::{self, BackpackItem};
use crate::services::inventory_service::{self, InventorySyncSummary};

#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct InventoryChanged {
    pub steam_id: String,
    pub total: u32,
}

#[tauri::command]
#[specta::specta]
pub async fn sync_inventory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<InventorySyncSummary> {
    let steam_id = state
        .config
        .read()
        .await
        .steam_id
        .ok_or_else(|| AppError::Config("not logged in with Steam".to_string()))?;

    let summary = inventory_service::sync(&state, steam_id).await?;

    let _ = InventoryChanged {
        steam_id: steam_id.to_string(),
        total: summary.total,
    }
    .emit(&app);

    Ok(summary)
}

#[tauri::command]
#[specta::specta]
pub async fn get_inventory(state: State<'_, AppState>) -> AppResult<Vec<BackpackItem>> {
    let steam_id = state
        .config
        .read()
        .await
        .steam_id
        .ok_or_else(|| AppError::Config("not logged in with Steam".to_string()))?;

    backpack_service::get_backpack(&state, steam_id).await
}
