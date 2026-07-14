use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::app::AppState;
use crate::domain::alerts::AlertKind;
use crate::error::{AppError, AppResult};
use crate::infra::db::repos::alerts_repo::{AlertEventsRepo, AlertRulesRepo, CreateAlertRule};
use crate::services::market_analyzer_service;

#[derive(Debug, Clone, Serialize, Type)]
pub struct AlertRuleView {
    pub id: i32,
    pub item_name: String,
    pub kind: String,
    pub threshold: Option<f64>,
    pub channels: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct AlertEventView {
    pub id: i32,
    pub rule_id: i32,
    pub item_name: String,
    pub kind: String,
    pub message: String,
    pub fired_ts: f64,
    pub acked: bool,
}

/// Mirrors the JSON `AlertService::record_and_dispatch` writes into
/// `alert_events.payload` — kept in sync by hand since the two live in
/// different modules (services::alert_service writes it, this reads it).
#[derive(Debug, Deserialize)]
struct AlertEventPayload {
    item_name: String,
    kind: String,
    message: String,
}

/// Creates an alert rule against the item a classifieds URL resolves to —
/// reuses `market_analyzer_service::resolve_item_id_from_url` (Module 7)
/// rather than requiring a separate item picker. Rejects up front (not
/// silently-never-fires) if `kind` needs a threshold and none was given.
#[tauri::command]
#[specta::specta]
pub async fn create_alert_rule(
    state: State<'_, AppState>,
    url: String,
    kind: String,
    threshold: Option<f64>,
    channels: Vec<String>,
) -> AppResult<i32> {
    let alert_kind = AlertKind::parse(&kind).map_err(|e| AppError::InvalidInput(e.to_string()))?;
    if alert_kind.requires_threshold() && threshold.is_none() {
        return Err(AppError::InvalidInput(format!(
            "'{kind}' alerts require a threshold"
        )));
    }

    let (item_id, _name) =
        market_analyzer_service::resolve_item_id_from_url(&state.db, &url).await?;

    let channels_json = serde_json::to_string(&channels)
        .map_err(|e| AppError::Internal(format!("failed to serialize channels: {e}")))?;

    AlertRulesRepo::create(
        &state.db,
        &CreateAlertRule {
            item_id,
            kind: alert_kind.as_str(),
            threshold,
            channels_json: &channels_json,
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn list_alert_rules(state: State<'_, AppState>) -> AppResult<Vec<AlertRuleView>> {
    let rows = AlertRulesRepo::list(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(|r| AlertRuleView {
            id: r.id,
            item_name: r.item_name,
            kind: r.kind,
            threshold: r.threshold,
            channels: serde_json::from_str(&r.channels).unwrap_or_default(),
            enabled: r.enabled,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn set_alert_rule_enabled(
    state: State<'_, AppState>,
    rule_id: i32,
    enabled: bool,
) -> AppResult<()> {
    AlertRulesRepo::set_enabled(&state.db, rule_id, enabled).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_alert_rule(state: State<'_, AppState>, rule_id: i32) -> AppResult<()> {
    AlertRulesRepo::delete(&state.db, rule_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_recent_alert_events(
    state: State<'_, AppState>,
    limit: u32,
) -> AppResult<Vec<AlertEventView>> {
    let rows = AlertEventsRepo::list_recent(&state.db, limit as i64).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let parsed: AlertEventPayload =
                serde_json::from_str(&r.payload).unwrap_or_else(|_| AlertEventPayload {
                    item_name: "Unknown Item".to_string(),
                    kind: "unknown".to_string(),
                    message: r.payload.clone(),
                });
            AlertEventView {
                id: r.id,
                rule_id: r.rule_id,
                item_name: parsed.item_name,
                kind: parsed.kind,
                message: parsed.message,
                fired_ts: r.fired_ts as f64,
                acked: r.acked,
            }
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn ack_alert_event(state: State<'_, AppState>, event_id: i32) -> AppResult<()> {
    AlertEventsRepo::ack(&state.db, event_id).await
}
