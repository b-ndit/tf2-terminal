use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::services::plugin_service::{self, PluginSummary};

/// Installs a plugin from a local directory containing `plugin.toml` and
/// its `entry` wasm file — the frontend shows the parsed manifest's
/// requested capabilities and asks the user to confirm before calling
/// this (`docs/DESIGN.md` §8: "user approves capabilities on install").
#[tauri::command]
#[specta::specta]
pub async fn install_plugin(
    state: State<'_, AppState>,
    source_dir: String,
) -> AppResult<PluginSummary> {
    plugin_service::install_plugin(
        &state.paths,
        &state.db,
        &state.plugin_runtime,
        &source_dir,
        now_unix(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn list_plugins(state: State<'_, AppState>) -> AppResult<Vec<PluginSummary>> {
    plugin_service::list_plugins(&state.db).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_plugin_enabled(
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> AppResult<()> {
    plugin_service::set_plugin_enabled(
        &state.db,
        &state.paths,
        &state.plugin_runtime,
        &name,
        enabled,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_plugin(state: State<'_, AppState>, name: String) -> AppResult<()> {
    plugin_service::uninstall_plugin(&state.db, &state.paths, &state.plugin_runtime, &name).await
}

/// `Some(path)` the frontend loads into a sandboxed
/// `<iframe sandbox="allow-scripts">` via `convertFileSrc` — `None` if the
/// plugin has no `panel/index.html`.
#[tauri::command]
#[specta::specta]
pub async fn get_plugin_panel_path(
    state: State<'_, AppState>,
    name: String,
) -> AppResult<Option<String>> {
    plugin_service::get_plugin_panel_path(&state.db, &state.paths, &name).await
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}
