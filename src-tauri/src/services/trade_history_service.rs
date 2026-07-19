//! Module 12: the completed-trade ledger. Promotes each completed offer's
//! cached Module 9 analysis into a permanent `trades` row — re-resolving
//! items from scratch after a trade completes isn't generally possible
//! (see the Module 12 note in `docs/DESIGN.md` §6: "given" items have left
//! the user's inventory, "received" items get new asset ids under the new
//! owner). A trade with no cached analysis (app wasn't open while it was
//! active) still gets a row, with its items marked unresolved rather than
//! silently missing from history.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::SqlitePool;

use crate::app::AppState;
use crate::domain::steam_id::SteamId64;
use crate::error::{AppError, AppResult};
use crate::infra::db::repos::kv_cache_repo::KvCacheRepo;
use crate::infra::db::repos::trades_repo::{InsertTrade, TradeRow, TradesRepo};
use crate::infra::keychain::{keys, Keychain};
use crate::infra::steam::trade_offers::{SteamTradeOfferClient, TradeOffer};
use crate::services::trade_analysis_engine::{
    trade_analysis_cache_key, CachedTradeAnalysis, TradeItemView,
};

/// Reused as generic bookkeeping state, not a true TTL'd cache entry —
/// long enough to never realistically expire.
const LAST_SYNC_CACHE_KEY: &str = "trade_history:last_sync_ts";
const LAST_SYNC_TTL: Duration = Duration::from_secs(10 * 365 * 24 * 3600);
/// First sync's bounded initial backfill — avoids pulling a fresh
/// install's entire multi-year Steam trade history by default.
const DEFAULT_BACKFILL_DAYS: i64 = 30;
const DAY_SECONDS: i64 = 86_400;
const UNRESOLVED_ITEM_NAME: &str = "Unresolved (not tracked while active)";

#[derive(Debug, Clone, Serialize, Type)]
pub struct TradeSyncSummary {
    pub checked: u32,
    pub imported: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LedgerItemView {
    pub name: String,
    pub value_ref: Option<f64>,
    /// Display-only fields carried over from Module 9's `TradeItemView` at
    /// import time. `#[serde(default)]` since `given_json`/`received_json`
    /// for trades imported before this was added won't have these keys at
    /// all — without it, deserializing an old row would fail outright and
    /// `row_to_view`'s `unwrap_or_default()` would silently blank out that
    /// trade's *entire* item list rather than just these new fields.
    #[serde(default)]
    pub quality: Option<u8>,
    #[serde(default)]
    pub effect_id: Option<u32>,
    #[serde(default)]
    pub killstreak_tier: Option<u8>,
    #[serde(default)]
    pub australium: Option<bool>,
    #[serde(default)]
    pub festivized: Option<bool>,
    #[serde(default)]
    pub paint_id: Option<i32>,
    #[serde(default)]
    pub craft_number: Option<i32>,
    #[serde(default)]
    pub strange_count: Option<i32>,
    #[serde(default)]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TradeLedgerView {
    pub trade_offer_id: String,
    pub partner_steam_id: String,
    pub completed_ts: f64,
    pub given: Vec<LedgerItemView>,
    pub received: Vec<LedgerItemView>,
    pub net_value_ref: f64,
    pub rating: Option<i32>,
    pub notes: Option<String>,
}

/// Fetches completed offers since the last sync (or `DEFAULT_BACKFILL_DAYS`
/// back, on a fresh install) and imports any not already in the ledger.
pub async fn sync_completed_trades(state: &AppState) -> AppResult<TradeSyncSummary> {
    let api_key = Keychain::get(keys::STEAM_API_KEY)?
        .ok_or_else(|| AppError::Config("Steam API key not set".to_string()))?;

    let since_ts = last_sync_ts(&state.db).await?;
    let client = SteamTradeOfferClient::new(&state.steam_api, api_key);
    let offers = client.fetch_completed_offers(since_ts).await?;

    let mut imported = 0u32;
    for offer in &offers {
        if import_completed_offer(&state.db, offer).await? {
            imported += 1;
        }
    }

    let now = now_unix();
    let _ = KvCacheRepo::set(
        &state.db,
        LAST_SYNC_CACHE_KEY,
        now.to_string().as_bytes(),
        LAST_SYNC_TTL,
    )
    .await;

    Ok(TradeSyncSummary {
        checked: offers.len() as u32,
        imported,
    })
}

async fn last_sync_ts(pool: &SqlitePool) -> AppResult<i64> {
    let default_cutoff = now_unix() - DEFAULT_BACKFILL_DAYS * DAY_SECONDS;
    let Some(bytes) = KvCacheRepo::get(pool, LAST_SYNC_CACHE_KEY).await? else {
        return Ok(default_cutoff);
    };
    Ok(String::from_utf8(bytes)
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default_cutoff))
}

/// Imports one completed offer, promoting its cached Module 9 analysis if
/// one exists. Returns whether it was newly inserted (`false` if this
/// trade was already in the ledger — `TradesRepo::insert_if_new` is
/// idempotent, so re-running a sync over an overlapping window is safe).
async fn import_completed_offer(pool: &SqlitePool, offer: &TradeOffer) -> AppResult<bool> {
    let trade_offer_id = offer.tradeofferid.to_string();
    let partner_steam_id = SteamId64::from_account_id(offer.accountid_other).to_string();

    let cache_key = trade_analysis_cache_key(&trade_offer_id);
    let cached: Option<CachedTradeAnalysis> = KvCacheRepo::get(pool, &cache_key)
        .await?
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let (given, received, net_value_ref) = match cached {
        Some(c) => (
            to_ledger_items(c.given),
            to_ledger_items(c.received),
            c.net_ref,
        ),
        None => (
            unresolved_items(offer.items_to_give.len()),
            unresolved_items(offer.items_to_receive.len()),
            0.0,
        ),
    };

    let given_json = serde_json::to_string(&given)
        .map_err(|e| AppError::Internal(format!("failed to serialize given items: {e}")))?;
    let received_json = serde_json::to_string(&received)
        .map_err(|e| AppError::Internal(format!("failed to serialize received items: {e}")))?;

    TradesRepo::insert_if_new(
        pool,
        &InsertTrade {
            trade_offer_id: &trade_offer_id,
            partner_steam_id: &partner_steam_id,
            completed_ts: offer.time_updated,
            given_json: &given_json,
            received_json: &received_json,
            net_value_ref,
        },
    )
    .await
}

fn to_ledger_items(items: Vec<TradeItemView>) -> Vec<LedgerItemView> {
    items
        .into_iter()
        .map(|v| LedgerItemView {
            name: v.name,
            value_ref: v.estimated_ref,
            quality: v.quality,
            effect_id: v.effect_id,
            killstreak_tier: v.killstreak_tier,
            australium: v.australium,
            festivized: v.festivized,
            paint_id: v.paint_id,
            craft_number: v.craft_number,
            strange_count: v.strange_count,
            image_url: v.image_url,
        })
        .collect()
}

fn unresolved_items(count: usize) -> Vec<LedgerItemView> {
    (0..count)
        .map(|_| LedgerItemView {
            name: UNRESOLVED_ITEM_NAME.to_string(),
            value_ref: None,
            quality: None,
            effect_id: None,
            killstreak_tier: None,
            australium: None,
            festivized: None,
            paint_id: None,
            craft_number: None,
            strange_count: None,
            image_url: None,
        })
        .collect()
}

pub async fn list_trades(pool: &SqlitePool, limit: i64) -> AppResult<Vec<TradeLedgerView>> {
    let rows = TradesRepo::list_recent(pool, limit).await?;
    Ok(rows.into_iter().map(row_to_view).collect())
}

fn row_to_view(row: TradeRow) -> TradeLedgerView {
    TradeLedgerView {
        trade_offer_id: row.trade_offer_id,
        partner_steam_id: row.partner_steam_id,
        completed_ts: row.completed_ts as f64,
        given: serde_json::from_str(&row.given_json).unwrap_or_default(),
        received: serde_json::from_str(&row.received_json).unwrap_or_default(),
        net_value_ref: row.net_value_ref,
        rating: row.rating.map(|r| r as i32),
        notes: row.notes,
    }
}

pub async fn set_trade_rating(
    pool: &SqlitePool,
    trade_offer_id: &str,
    rating: Option<i32>,
) -> AppResult<()> {
    TradesRepo::set_rating(pool, trade_offer_id, rating.map(|r| r as i64)).await
}

pub async fn set_trade_notes(
    pool: &SqlitePool,
    trade_offer_id: &str,
    notes: Option<&str>,
) -> AppResult<()> {
    TradesRepo::set_notes(pool, trade_offer_id, notes).await
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
#[path = "trade_history_service_tests.rs"]
mod tests;
