//! Module 12: whole-backpack valuation snapshots, P/L windows, and
//! winners/losers. Reuses Module 11's shared `item_valuation::value_item_key`
//! (grouped by *distinct* `item_id` first, so a stack of identical items —
//! 5 Refined Metal, say — only gets valued once and multiplied by count)
//! and `domain::trend::trend` directly for winners/losers (no new scoring
//! needed there, just a per-owned-item loop + sort). The only new pure
//! domain code is `domain::portfolio::pl_window`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::domain::portfolio::pl_window;
use crate::domain::trend::{self, PricePoint};
use crate::error::AppResult;
use crate::infra::config::Config;
use crate::infra::db::repos::inventory_repo::{InventoryItemView, InventoryRepo};
use crate::infra::db::repos::portfolio_repo::{InsertPortfolioSnapshot, PortfolioSnapshotsRepo};
use crate::infra::db::repos::price_history_repo::PriceDailyRepo;
use crate::services::item_valuation::value_item_key;

const DAY_SECONDS: i64 = 86_400;

// Currency item defindexes (Valve schema, stable since ~2011) — "pure"
// holdings tallied separately from itemized value, same as
// `history_recorder.rs`'s `MANN_CO_KEY_DEFINDEX` convention (each file
// that needs one of these defines its own constant rather than sharing).
const DEFINDEX_KEY: i64 = 5021;
const DEFINDEX_REFINED: i64 = 5002;
const DEFINDEX_RECLAIMED: i64 = 5001;
const DEFINDEX_SCRAP: i64 = 5000;
/// TF2's metal denominations, in ref: 9 Scrap = 3 Reclaimed = 1 Refined.
const RECLAIMED_REF: f64 = 1.0 / 3.0;
const SCRAP_REF: f64 = 1.0 / 9.0;

#[derive(Debug, Clone, Serialize, Type)]
pub struct PortfolioSnapshotView {
    pub ts: f64,
    pub total_ref: f64,
    pub total_keys: f64,
    pub pure_keys: u32,
    pub pure_metal_ref: f64,
    pub item_count: u32,
    pub unusual_count: u32,
    pub australium_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
pub struct PlWindowView {
    pub abs_ref: f64,
    pub pct: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
pub struct PlWindowsView {
    pub d1: Option<PlWindowView>,
    pub d7: Option<PlWindowView>,
    pub d30: Option<PlWindowView>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ItemMoverView {
    pub item_name: String,
    pub count: u32,
    /// Current per-unit value; `None` if this item couldn't be priced at
    /// all (matches the "unresolved" convention elsewhere — Modules 9/11).
    pub current_value_ref: Option<f64>,
    pub change_pct: Option<f64>,
}

/// Values every distinct item currently in `steam_id`'s inventory, tallies
/// pure currency/unusual/australium counts, and persists + returns the
/// snapshot. Called both by the daily periodic loop and on-demand (a
/// "Refresh" button) — both paths converge here, matching §6's "daily and
/// on-demand valuation snapshots".
pub async fn snapshot_now(
    pool: &SqlitePool,
    steam_id: &str,
    now_ts: i64,
) -> AppResult<PortfolioSnapshotView> {
    let items = InventoryRepo::list_with_items(pool, steam_id).await?;

    let mut grouped: HashMap<i64, (InventoryItemView, u32)> = HashMap::new();
    for item in items {
        let item_id = item.item_id;
        grouped
            .entry(item_id)
            .and_modify(|(_, count)| *count += 1)
            .or_insert_with(|| (item, 1));
    }

    let mut total_ref = 0.0;
    let mut pure_keys = 0u32;
    let mut pure_metal_ref = 0.0;
    let mut item_count = 0u32;
    let mut unusual_count = 0u32;
    let mut australium_count = 0u32;

    for (item, count) in grouped.values() {
        item_count += count;
        if item.quality == Quality::Unusual as i64 {
            unusual_count += count;
        }
        if item.australium {
            australium_count += count;
        }

        match item.defindex {
            DEFINDEX_KEY => {
                pure_keys += count;
                continue; // valued via total_keys below, not total_ref
            }
            DEFINDEX_REFINED => {
                pure_metal_ref += *count as f64;
                continue;
            }
            DEFINDEX_RECLAIMED => {
                pure_metal_ref += *count as f64 * RECLAIMED_REF;
                continue;
            }
            DEFINDEX_SCRAP => {
                pure_metal_ref += *count as f64 * SCRAP_REF;
                continue;
            }
            _ => {}
        }

        let Ok(key) = item_key_from_view(item) else {
            continue;
        };
        let valuation = value_item_key(pool, &key, &item.name, now_ts).await?;
        if let Some(value_ref) = valuation.estimated_ref.or(valuation.quicksell_ref) {
            total_ref += value_ref * *count as f64;
        }
    }

    // Pure keys value at whatever the going key rate implies — reuse the
    // Key's own market valuation rather than re-deriving a rate, so this
    // stays consistent with everything else in `total_ref`.
    let key_item_key = ItemKey {
        defindex: DEFINDEX_KEY as u32,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    let key_valuation =
        value_item_key(pool, &key_item_key, "Mann Co. Supply Crate Key", now_ts).await?;
    let key_value_ref = key_valuation
        .estimated_ref
        .or(key_valuation.quicksell_ref)
        .unwrap_or(0.0);
    total_ref += key_value_ref * pure_keys as f64;
    total_ref += pure_metal_ref;

    let total_keys = if key_value_ref > 0.0 {
        total_ref / key_value_ref
    } else {
        0.0
    };

    PortfolioSnapshotsRepo::insert(
        pool,
        &InsertPortfolioSnapshot {
            ts: now_ts,
            steam_id,
            total_ref,
            total_keys,
            pure_keys: Some(pure_keys as i64),
            pure_metal_ref: Some(pure_metal_ref),
            item_count: Some(item_count as i64),
            unusual_count: Some(unusual_count as i64),
            australium_count: Some(australium_count as i64),
        },
    )
    .await?;

    Ok(PortfolioSnapshotView {
        ts: now_ts as f64,
        total_ref,
        total_keys,
        pure_keys,
        pure_metal_ref,
        item_count,
        unusual_count,
        australium_count,
    })
}

fn item_key_from_view(item: &InventoryItemView) -> Result<ItemKey, crate::domain::item::ItemError> {
    Ok(ItemKey {
        defindex: item.defindex as u32,
        quality: Quality::try_from(item.quality as u8)?,
        effect_id: item.effect_id.map(|e| e as u32),
        killstreak_tier: KillstreakTier::try_from(item.killstreak_tier as u8)?,
        australium: item.australium,
        festivized: item.festivized,
        craftable: item.craftable,
    })
}

pub async fn get_portfolio_history(
    pool: &SqlitePool,
    steam_id: &str,
    since_ts: i64,
) -> AppResult<Vec<PortfolioSnapshotView>> {
    let rows = PortfolioSnapshotsRepo::history_since(pool, steam_id, since_ts).await?;
    Ok(rows
        .into_iter()
        .map(|r| PortfolioSnapshotView {
            ts: r.ts as f64,
            total_ref: r.total_ref,
            total_keys: r.total_keys,
            pure_keys: r.pure_keys.unwrap_or(0) as u32,
            pure_metal_ref: r.pure_metal_ref.unwrap_or(0.0),
            item_count: r.item_count.unwrap_or(0) as u32,
            unusual_count: r.unusual_count.unwrap_or(0) as u32,
            australium_count: r.australium_count.unwrap_or(0) as u32,
        })
        .collect())
}

/// 1/7/30-day P/L, each `None` if there's no snapshot far enough back to
/// compare against yet (e.g. a fresh install).
pub async fn get_pl_windows(
    pool: &SqlitePool,
    steam_id: &str,
    now_ts: i64,
) -> AppResult<PlWindowsView> {
    let Some(current) = PortfolioSnapshotsRepo::latest(pool, steam_id).await? else {
        return Ok(PlWindowsView {
            d1: None,
            d7: None,
            d30: None,
        });
    };

    let mut windows = [None, None, None];
    for (i, days) in [1_i64, 7, 30].into_iter().enumerate() {
        let past_ts = now_ts - days * DAY_SECONDS;
        if let Some(past) =
            PortfolioSnapshotsRepo::most_recent_before(pool, steam_id, past_ts).await?
        {
            windows[i] = pl_window(current.total_ref, past.total_ref).map(|w| PlWindowView {
                abs_ref: w.abs_ref,
                pct: w.pct,
            });
        }
    }

    Ok(PlWindowsView {
        d1: windows[0],
        d7: windows[1],
        d30: windows[2],
    })
}

/// Every distinct item currently owned, ranked by its `price_daily` trend
/// over `window_days` (1, 7, or 30) — sorted best-to-worst % change so the
/// frontend can slice the top/bottom N for "winners"/"losers" without a
/// second round trip.
pub async fn get_winners_losers(
    pool: &SqlitePool,
    steam_id: &str,
    window_days: i64,
    now_ts: i64,
) -> AppResult<Vec<ItemMoverView>> {
    let items = InventoryRepo::list_with_items(pool, steam_id).await?;

    let mut grouped: HashMap<i64, (InventoryItemView, u32)> = HashMap::new();
    for item in items {
        let item_id = item.item_id;
        grouped
            .entry(item_id)
            .and_modify(|(_, count)| *count += 1)
            .or_insert_with(|| (item, 1));
    }

    let mut movers = Vec::with_capacity(grouped.len());
    for (item_id, (item, count)) in &grouped {
        let history = daily_history(pool, *item_id).await?;
        let change_pct = match window_days {
            1 => trend::trend(&history, now_ts).d1,
            7 => trend::trend(&history, now_ts).d7,
            _ => trend::trend(&history, now_ts).d30,
        };
        let current_value_ref = history
            .iter()
            .filter(|p| p.ts <= now_ts)
            .max_by_key(|p| p.ts)
            .map(|p| p.value_ref);

        movers.push(ItemMoverView {
            item_name: item.name.clone(),
            count: *count,
            current_value_ref,
            change_pct,
        });
    }

    movers.sort_by(|a, b| {
        b.change_pct
            .partial_cmp(&a.change_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(movers)
}

async fn daily_history(pool: &SqlitePool, item_id: i64) -> AppResult<Vec<PricePoint>> {
    let rows = PriceDailyRepo::history_since(pool, item_id, 0).await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            r.close_ref.map(|value_ref| PricePoint {
                ts: r.day * DAY_SECONDS,
                value_ref,
            })
        })
        .collect())
}

/// Spawns the daily snapshot loop for the process's lifetime. No-ops
/// (skips that tick) while nobody's logged in — `spawn_periodic_snapshot`
/// itself never fails, it just has nothing to snapshot yet.
pub fn spawn_periodic_snapshot(config: Arc<RwLock<Config>>, db: SqlitePool, interval: Duration) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let steam_id = config.read().await.steam_id;
            let Some(steam_id) = steam_id else { continue };
            let now_ts = now_unix();
            if let Err(e) = snapshot_now(&db, &steam_id.to_string(), now_ts).await {
                tracing::warn!(error = %e, "periodic portfolio snapshot failed");
            }
        }
    });
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
#[path = "portfolio_service_tests.rs"]
mod tests;
