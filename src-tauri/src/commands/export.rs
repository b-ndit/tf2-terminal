use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::infra::export::ExportFormat;
use crate::services::export_service;

/// Writes the current backpack to `path` in `format`. `path` is a value
/// the frontend already obtained from `tauri-plugin-dialog`'s native save
/// picker — this command only ever writes to a location the user picked.
#[tauri::command]
#[specta::specta]
pub async fn export_backpack(
    state: State<'_, AppState>,
    format: ExportFormat,
    path: String,
) -> AppResult<()> {
    export_service::export_backpack(&state, format, &path).await
}

#[tauri::command]
#[specta::specta]
pub async fn export_trade_history(
    state: State<'_, AppState>,
    format: ExportFormat,
    path: String,
) -> AppResult<()> {
    export_service::export_trade_history(&state, format, &path).await
}

#[tauri::command]
#[specta::specta]
pub async fn export_portfolio(
    state: State<'_, AppState>,
    format: ExportFormat,
    path: String,
) -> AppResult<()> {
    export_service::export_portfolio(&state, format, &path).await
}
