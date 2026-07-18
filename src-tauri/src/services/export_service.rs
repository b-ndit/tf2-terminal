//! Module 15: assembles each dataset's already-existing service/repo data
//! into the tabular shape `infra::export`'s CSV/XLSX/PDF writers need;
//! JSON export bypasses that and serializes the structured data directly
//! (see `infra::export::ExportTable`'s doc comment for why). The actual
//! `std::fs::write` to the path the frontend obtained via
//! `tauri-plugin-dialog`'s save picker is the one piece of real I/O here.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::app::AppState;
use crate::domain::item::Quality;
use crate::error::{AppError, AppResult};
use crate::infra::export::{csv, pdf, xlsx, ExportFormat, ExportTable};
use crate::services::backpack_service::{self, BackpackItem};
use crate::services::portfolio_service::{self, PortfolioSnapshotView};
use crate::services::trade_history_service::{self, LedgerItemView, TradeLedgerView};

/// Generous cap matching other "export everything" list calls elsewhere
/// (e.g. `commands::trade_history::list_trades`'s frontend-supplied
/// limit) — high enough that no realistic collection gets truncated.
const HISTORY_LIMIT: i64 = 10_000;

pub async fn export_backpack(state: &AppState, format: ExportFormat, path: &str) -> AppResult<()> {
    let steam_id = state
        .config
        .read()
        .await
        .steam_id
        .ok_or_else(|| AppError::Config("not logged in with Steam".to_string()))?;
    let items = backpack_service::get_backpack(state, steam_id).await?;
    write_export(format, path, "Backpack Export", &items, backpack_table)
}

pub async fn export_trade_history(
    state: &AppState,
    format: ExportFormat,
    path: &str,
) -> AppResult<()> {
    let trades = trade_history_service::list_trades(&state.db, HISTORY_LIMIT).await?;
    write_export(
        format,
        path,
        "Trade History Export",
        &trades,
        trade_history_table,
    )
}

pub async fn export_portfolio(state: &AppState, format: ExportFormat, path: &str) -> AppResult<()> {
    let steam_id = state
        .config
        .read()
        .await
        .steam_id
        .ok_or_else(|| AppError::Config("not logged in with Steam".to_string()))?;
    let history =
        portfolio_service::get_portfolio_history(&state.db, &steam_id.to_string(), 0).await?;
    write_export(format, path, "Portfolio Export", &history, portfolio_table)
}

/// Shared across all three datasets: JSON serializes `data` as-is; the
/// other three formats go through `to_table` first, then the matching
/// `infra::export` writer.
fn write_export<T: Serialize>(
    format: ExportFormat,
    path: &str,
    title: &str,
    data: &[T],
    to_table: impl Fn(&[T]) -> ExportTable,
) -> AppResult<()> {
    let bytes = match format {
        ExportFormat::Json => {
            serde_json::to_vec_pretty(data).map_err(|e| AppError::Export(e.to_string()))?
        }
        ExportFormat::Csv => csv::write(&to_table(data))?,
        ExportFormat::Xlsx => xlsx::write(&to_table(data))?,
        ExportFormat::Pdf => pdf::write(title, &to_table(data))?,
    };
    std::fs::write(path, bytes).map_err(AppError::from)
}

fn backpack_table(items: &[BackpackItem]) -> ExportTable {
    let headers = [
        "Name",
        "Quality",
        "Effect ID",
        "Killstreak Tier",
        "Australium",
        "Festivized",
        "Craftable",
        "Craft Number",
        "Paint ID",
        "Strange Count",
        "Tradable",
        "Marketable",
        "Folder",
        "Favorite",
        "Pinned",
        "Note",
        "Custom Label",
        "Tags",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let rows = items
        .iter()
        .map(|item| {
            vec![
                item.name.clone(),
                Quality::try_from(item.quality)
                    .map(|q| q.display_name().to_string())
                    .unwrap_or_else(|_| item.quality.to_string()),
                opt(item.effect_id),
                item.killstreak_tier.to_string(),
                bool_str(item.australium),
                bool_str(item.festivized),
                bool_str(item.craftable),
                opt(item.craft_number),
                opt(item.paint_id),
                opt(item.strange_count),
                bool_str(item.tradable),
                item.marketable.map(bool_str).unwrap_or_default(),
                item.meta.folder.clone().unwrap_or_default(),
                bool_str(item.meta.favorite),
                bool_str(item.meta.pinned),
                item.meta.note.clone().unwrap_or_default(),
                item.meta.custom_label.clone().unwrap_or_default(),
                item.tags
                    .iter()
                    .map(|t| t.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            ]
        })
        .collect();

    ExportTable { headers, rows }
}

fn trade_history_table(trades: &[TradeLedgerView]) -> ExportTable {
    let headers = [
        "Completed At",
        "Partner Steam ID",
        "Given",
        "Received",
        "Net Value (ref)",
        "Rating",
        "Notes",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let rows = trades
        .iter()
        .map(|t| {
            vec![
                format_ts(t.completed_ts),
                t.partner_steam_id.clone(),
                join_ledger_items(&t.given),
                join_ledger_items(&t.received),
                format!("{:.2}", t.net_value_ref),
                t.rating.map(|r| r.to_string()).unwrap_or_default(),
                t.notes.clone().unwrap_or_default(),
            ]
        })
        .collect();

    ExportTable { headers, rows }
}

fn join_ledger_items(items: &[LedgerItemView]) -> String {
    items
        .iter()
        .map(|i| match i.value_ref {
            Some(v) => format!("{} ({v:.2} ref)", i.name),
            None => i.name.clone(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn portfolio_table(snapshots: &[PortfolioSnapshotView]) -> ExportTable {
    let headers = [
        "Date",
        "Total (ref)",
        "Total (keys)",
        "Pure Keys",
        "Pure Metal (ref)",
        "Item Count",
        "Unusual Count",
        "Australium Count",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let rows = snapshots
        .iter()
        .map(|s| {
            vec![
                format_ts(s.ts),
                format!("{:.2}", s.total_ref),
                format!("{:.2}", s.total_keys),
                s.pure_keys.to_string(),
                format!("{:.2}", s.pure_metal_ref),
                s.item_count.to_string(),
                s.unusual_count.to_string(),
                s.australium_count.to_string(),
            ]
        })
        .collect();

    ExportTable { headers, rows }
}

fn format_ts(ts: f64) -> String {
    DateTime::<Utc>::from_timestamp(ts as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn bool_str(b: bool) -> String {
    if b { "Yes" } else { "No" }.to_string()
}

#[cfg(test)]
#[path = "export_service_tests.rs"]
mod tests;
