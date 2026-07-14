use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::services::schema_service::{self, SchemaSyncSummary};

#[tauri::command]
#[specta::specta]
pub async fn sync_item_schema(state: State<'_, AppState>) -> AppResult<SchemaSyncSummary> {
    schema_service::sync(&state).await
}
