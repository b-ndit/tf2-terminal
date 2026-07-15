//! Module 14's plugin lifecycle: install/enable/uninstall, dispatching
//! `AlertFired` events to subscribed plugins, and polling `market_provider`
//! plugins for listings to feed into the shared market data bus. Ties
//! together `domain::plugin` (manifest parsing), `infra::db::repos::
//! plugins_repo` (persisted metadata), and `infra::plugins::runtime`
//! (the wasmtime host) — see the Module 14 implementation note in
//! `docs/DESIGN.md` for the overall design.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;

use crate::domain::plugin::{self, Capability};
use crate::error::{AppError, AppResult};
use crate::infra::backpack_tf::models::ListingEvent;
use crate::infra::config::AppPaths;
use crate::infra::db::repos::plugins_repo::{InsertPlugin, PluginRow, PluginsRepo};
use crate::infra::plugins::runtime::{HostContextParams, PluginRuntime};
use crate::services::alert_service::AlertFired;
use crate::services::market_data_service::MarketDataService;

#[derive(Debug, Clone, Serialize, Type)]
pub struct PluginSummary {
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub events: Vec<String>,
    pub has_panel: bool,
    pub enabled: bool,
    pub installed_ts: f64,
}

fn row_to_summary(row: PluginRow) -> AppResult<PluginSummary> {
    let capabilities = serde_json::from_str(&row.capabilities_json)
        .map_err(|e| AppError::Internal(format!("corrupt capabilities_json: {e}")))?;
    let events = serde_json::from_str(&row.events_json)
        .map_err(|e| AppError::Internal(format!("corrupt events_json: {e}")))?;
    Ok(PluginSummary {
        name: row.name,
        version: row.version,
        capabilities,
        events,
        has_panel: row.has_panel,
        enabled: row.enabled,
        installed_ts: row.installed_ts as f64,
    })
}

fn parse_granted_capabilities(capabilities_json: &str) -> Vec<Capability> {
    serde_json::from_str::<Vec<String>>(capabilities_json)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| Capability::parse(s).ok())
        .collect()
}

fn subscribes_to(events_json: &str, event: &str) -> bool {
    serde_json::from_str::<Vec<String>>(events_json)
        .unwrap_or_default()
        .iter()
        .any(|e| e == event)
}

/// Recursively copies `src` into `dst` (creating `dst`). Plain sync I/O —
/// installing a plugin is a rare, user-initiated action on a handful of
/// small files, not a hot path worth threading through `spawn_blocking`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

/// Parses `source_dir/plugin.toml`, copies the whole directory into
/// `data_dir/plugins/<name>/`, compiles it into `runtime`'s module cache,
/// and records it (enabled) in the `plugins` table. Capability approval is
/// all-or-nothing at install time (`docs/DESIGN.md` §8) — the frontend
/// shows the manifest's requested capabilities before calling this.
pub async fn install_plugin(
    paths: &AppPaths,
    db: &SqlitePool,
    runtime: &PluginRuntime,
    source_dir: &str,
    now_ts: i64,
) -> AppResult<PluginSummary> {
    let source_dir = Path::new(source_dir);
    let manifest_path = source_dir.join("plugin.toml");
    let manifest_str = std::fs::read_to_string(&manifest_path)
        .map_err(|e| AppError::InvalidInput(format!("failed to read plugin.toml: {e}")))?;
    let manifest =
        plugin::parse_manifest(&manifest_str).map_err(|e| AppError::InvalidInput(e.to_string()))?;

    let entry_source = source_dir.join(&manifest.entry);
    if !entry_source.is_file() {
        return Err(AppError::InvalidInput(format!(
            "entry file '{}' not found in {}",
            manifest.entry,
            source_dir.display()
        )));
    }

    let install_dir = paths.plugins_dir().join(&manifest.name);
    if install_dir.exists() {
        std::fs::remove_dir_all(&install_dir)?;
    }
    copy_dir_recursive(source_dir, &install_dir)?;

    let has_panel = install_dir.join("panel").join("index.html").is_file();
    let capabilities: Vec<String> = manifest
        .capabilities
        .iter()
        .map(Capability::as_string)
        .collect();
    let events: Vec<String> = manifest
        .events
        .iter()
        .map(|e| e.as_string().to_string())
        .collect();
    let capabilities_json = serde_json::to_string(&capabilities)
        .map_err(|e| AppError::Internal(format!("failed to serialize capabilities: {e}")))?;
    let events_json = serde_json::to_string(&events)
        .map_err(|e| AppError::Internal(format!("failed to serialize events: {e}")))?;

    if PluginsRepo::find_by_name(db, &manifest.name)
        .await?
        .is_some()
    {
        PluginsRepo::delete(db, &manifest.name).await?;
        runtime.unload(&manifest.name);
    }

    PluginsRepo::insert(
        db,
        &InsertPlugin {
            name: &manifest.name,
            version: &manifest.version,
            entry_file: &manifest.entry,
            capabilities_json: &capabilities_json,
            events_json: &events_json,
            has_panel,
            installed_ts: now_ts,
        },
    )
    .await?;

    let wasm_path = install_dir.join(&manifest.entry);
    runtime.load(&manifest.name, &wasm_path)?;

    Ok(PluginSummary {
        name: manifest.name,
        version: manifest.version,
        capabilities,
        events,
        has_panel,
        enabled: true,
        installed_ts: now_ts as f64,
    })
}

pub async fn list_plugins(db: &SqlitePool) -> AppResult<Vec<PluginSummary>> {
    PluginsRepo::list(db)
        .await?
        .into_iter()
        .map(row_to_summary)
        .collect()
}

/// Toggling `enabled` loads/unloads the runtime's compiled-module cache to
/// match — a disabled plugin is never instantiated, not just skipped at
/// dispatch time.
pub async fn set_plugin_enabled(
    db: &SqlitePool,
    paths: &AppPaths,
    runtime: &PluginRuntime,
    name: &str,
    enabled: bool,
) -> AppResult<()> {
    PluginsRepo::set_enabled(db, name, enabled).await?;
    if enabled {
        if let Some(row) = PluginsRepo::find_by_name(db, name).await? {
            let wasm_path = paths.plugins_dir().join(name).join(&row.entry_file);
            runtime.load(name, &wasm_path)?;
        }
    } else {
        runtime.unload(name);
    }
    Ok(())
}

pub async fn uninstall_plugin(
    db: &SqlitePool,
    paths: &AppPaths,
    runtime: &PluginRuntime,
    name: &str,
) -> AppResult<()> {
    PluginsRepo::delete(db, name).await?;
    runtime.unload(name);
    let install_dir = paths.plugins_dir().join(name);
    if install_dir.exists() {
        std::fs::remove_dir_all(&install_dir)?;
    }
    Ok(())
}

/// `Some(path)` to `panel/index.html` inside the plugin's install
/// directory, for the frontend to load into a sandboxed iframe via
/// Tauri's asset protocol — `None` if the plugin has no panel or isn't
/// installed.
pub async fn get_plugin_panel_path(
    db: &SqlitePool,
    paths: &AppPaths,
    name: &str,
) -> AppResult<Option<String>> {
    let Some(row) = PluginsRepo::find_by_name(db, name).await? else {
        return Ok(None);
    };
    if !row.has_panel {
        return Ok(None);
    }
    let panel_path = paths
        .plugins_dir()
        .join(name)
        .join("panel")
        .join("index.html");
    if !panel_path.is_file() {
        return Ok(None);
    }
    Ok(Some(panel_path.to_string_lossy().into_owned()))
}

/// Loads every already-installed, enabled plugin into `runtime`'s module
/// cache — called once from `app::build()` so a restart doesn't require
/// re-installing anything. A single plugin failing to load is logged and
/// skipped rather than aborting startup.
pub async fn load_enabled_plugins(
    paths: &AppPaths,
    db: &SqlitePool,
    runtime: &PluginRuntime,
) -> AppResult<()> {
    for row in PluginsRepo::list_enabled(db).await? {
        let wasm_path = paths.plugins_dir().join(&row.name).join(&row.entry_file);
        if let Err(e) = runtime.load(&row.name, &wasm_path) {
            tracing::warn!(error = %e, plugin = %row.name, "failed to load plugin at startup");
        }
    }
    Ok(())
}

/// Best-effort: calls `on_alert_fired` on every enabled plugin subscribed
/// to it (`events` contains `"alert_fired"`). A single plugin trapping or
/// erroring is logged and doesn't stop the others or the caller
/// (`alert_service`'s own dispatch loop, which has real alerts left to
/// deliver through its own sinks either way).
pub async fn dispatch_alert_to_plugins(
    db: &SqlitePool,
    runtime: &Arc<PluginRuntime>,
    http: &reqwest::Client,
    event: &AlertFired,
) {
    let rows = match PluginsRepo::list_enabled(db).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "failed to list plugins for alert dispatch");
            return;
        }
    };
    let Ok(payload) = serde_json::to_vec(event) else {
        return;
    };

    for row in rows {
        if !subscribes_to(&row.events_json, "alert_fired") {
            continue;
        }
        let granted = parse_granted_capabilities(&row.capabilities_json);
        let params = HostContextParams {
            granted,
            db: db.clone(),
            http: http.clone(),
            tokio_handle: tokio::runtime::Handle::current(),
            steam_id: None,
        };
        let runtime = runtime.clone();
        let name = row.name.clone();
        let payload = payload.clone();
        let result = tokio::task::spawn_blocking(move || {
            runtime.call_export_json(&name, params, "on_alert_fired", Some(&payload))
        })
        .await;
        match result {
            Ok(Err(e)) => {
                tracing::warn!(error = %e, plugin = %row.name, "plugin alert dispatch failed")
            }
            Err(e) => {
                tracing::warn!(error = %e, plugin = %row.name, "plugin alert dispatch task panicked")
            }
            Ok(Ok(_)) => {}
        }
    }
}

/// Polls every enabled plugin subscribed to `"market_provider"` for a
/// fresh batch of listings (`provide_listings`), injecting whatever comes
/// back into `market_data`'s shared broadcast bus — the same one Live
/// Feed/History Recorder/Flip Finder/Alerts already consume, so none of
/// them need to change to pick up plugin-sourced listings.
async fn poll_market_providers(
    db: &SqlitePool,
    runtime: &Arc<PluginRuntime>,
    http: &reqwest::Client,
    market_data: &Arc<MarketDataService>,
) {
    let rows = match PluginsRepo::list_enabled(db).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "failed to list plugins for market provider poll");
            return;
        }
    };

    for row in rows {
        if !subscribes_to(&row.events_json, "market_provider") {
            continue;
        }
        let granted = parse_granted_capabilities(&row.capabilities_json);
        let params = HostContextParams {
            granted,
            db: db.clone(),
            http: http.clone(),
            tokio_handle: tokio::runtime::Handle::current(),
            steam_id: None,
        };
        let runtime = runtime.clone();
        let name = row.name.clone();
        let result = tokio::task::spawn_blocking(move || {
            runtime.call_export_json(&name, params, "provide_listings", None)
        })
        .await;
        let bytes = match result {
            Ok(Ok(Some(bytes))) => bytes,
            Ok(Ok(None)) => continue,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, plugin = %row.name, "market provider poll failed");
                continue;
            }
            Err(e) => {
                tracing::warn!(error = %e, plugin = %row.name, "market provider poll task panicked");
                continue;
            }
        };
        match serde_json::from_slice::<Vec<ListingEvent>>(&bytes) {
            Ok(events) => market_data.inject_external_listings(db, events).await,
            Err(e) => {
                tracing::warn!(error = %e, plugin = %row.name, "provide_listings returned malformed JSON")
            }
        }
    }
}

pub fn spawn_market_provider_poll(
    db: SqlitePool,
    runtime: Arc<PluginRuntime>,
    http: reqwest::Client,
    market_data: Arc<MarketDataService>,
    interval: Duration,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            poll_market_providers(&db, &runtime, &http, &market_data).await;
        }
    });
}

#[cfg(test)]
#[path = "plugin_service_tests.rs"]
mod tests;
