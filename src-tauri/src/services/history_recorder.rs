//! Builds `price_points`/`price_daily` (`docs/DESIGN.md` §5/§6) from two
//! sources: periodic snapshots of our own `market_listings` state
//! (`source = "snapshot"`), and one observation per catalog item whenever
//! the community price catalog syncs (`source = "schema"`). See the
//! Module 8 implementation note in `docs/DESIGN.md` §6 for why there's no
//! literal per-websocket-event row.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;

use crate::domain::currency::{Currency, KeyRate};
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::error::AppResult;
use crate::infra::backpack_tf::models::{CraftableEntry, PriceCatalogResponse, PriceEntry};
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::MarketListingsRepo;
use crate::infra::db::repos::price_history_repo::{
    mid_value, InsertPricePoint, PriceDailyRepo, PricePointsRepo,
};

const DAY_SECONDS: i64 = 86_400;
/// Valve schema defindex for "Mann Co. Supply Crate Key" — used to derive
/// the going key↔ref rate from whichever source (live listings or the
/// catalog) is on hand, since nothing else in this app tracks that rate yet.
const MANN_CO_KEY_DEFINDEX: u32 = 5021;

pub struct HistoryRecorder;

impl HistoryRecorder {
    /// Snapshots every currently-listed item's buy/sell depth into
    /// `price_points`, then recomputes today's `price_daily` bar for each.
    /// No-ops (returns `Ok(0)`) if no key rate can be derived yet — that
    /// only happens very early after boot, before the websocket has seen
    /// any Key listings at all.
    pub async fn snapshot(pool: &SqlitePool) -> AppResult<u32> {
        let aggregates = MarketListingsRepo::aggregate_by_item(pool).await?;

        let Some(key_rate_ref) = aggregates
            .iter()
            .find(|a| {
                a.defindex == MANN_CO_KEY_DEFINDEX as i64
                    && a.quality == Quality::Unique as u8 as i64
                    && a.killstreak_tier == 0
                    && !a.australium
                    && !a.festivized
                    && a.craftable
            })
            .and_then(|key_row| mid_value(key_row.best_buy_ref, key_row.best_sell_ref))
        else {
            return Ok(0);
        };

        let now = now_unix();
        let today = now / DAY_SECONDS;
        let mut recorded = 0u32;

        for agg in &aggregates {
            let Ok(quality) = Quality::try_from(agg.quality as u8) else {
                continue;
            };
            let Ok(killstreak_tier) = KillstreakTier::try_from(agg.killstreak_tier as u8) else {
                continue;
            };
            let key = ItemKey {
                defindex: agg.defindex as u32,
                quality,
                effect_id: agg.effect_id.map(|e| e as u32),
                killstreak_tier,
                australium: agg.australium,
                festivized: agg.festivized,
                craftable: agg.craftable,
            };
            let Some(item_id) = ItemsRepo::find_id_by_key(pool, &key).await? else {
                continue;
            };

            PricePointsRepo::insert(
                pool,
                &InsertPricePoint {
                    item_id,
                    ts: now,
                    source: "snapshot",
                    best_buy_keys: None,
                    best_buy_ref: agg.best_buy_ref,
                    best_sell_keys: None,
                    best_sell_ref: agg.best_sell_ref,
                    buy_count: Some(agg.buy_count),
                    sell_count: Some(agg.sell_count),
                    key_rate_ref,
                },
            )
            .await?;
            PriceDailyRepo::recompute_day(pool, item_id, today).await?;
            recorded += 1;
        }

        Ok(recorded)
    }

    /// Records one `source = "schema"` observation per catalog entry this
    /// app already has a display name for. Only the `Tradable` price group
    /// is used — `ItemKey` has no tradable flag, so mixing in `Non-Tradable`
    /// prices would misrepresent a tradable item's history. The catalog
    /// itself carries no killstreak/australium/festivized breakdown, so
    /// these observations are necessarily the base (non-killstreak,
    /// non-australium, non-festivized) permutation only — the snapshot path
    /// above is what captures those variants.
    pub async fn record_schema_sync(
        pool: &SqlitePool,
        catalog: &PriceCatalogResponse,
    ) -> AppResult<u32> {
        let Some(key_rate) = find_key_rate(catalog) else {
            return Ok(0);
        };

        let now = now_unix();
        let today = now / DAY_SECONDS;
        let mut recorded = 0u32;

        for catalog_item in catalog.items.values() {
            for (quality_str, quality_prices) in &catalog_item.prices {
                let Ok(quality_id) = quality_str.parse::<u8>() else {
                    continue;
                };
                let Ok(quality) = Quality::try_from(quality_id) else {
                    continue;
                };
                let Some(tradable) = &quality_prices.tradable else {
                    continue;
                };

                for (craftable, group) in [
                    (true, &tradable.craftable),
                    (false, &tradable.non_craftable),
                ] {
                    let Some(group) = group else { continue };
                    for &defindex_i64 in &catalog_item.defindex {
                        let Ok(defindex) = u32::try_from(defindex_i64) else {
                            continue;
                        };
                        recorded += record_catalog_group(
                            pool, defindex, quality, craftable, group, key_rate, now, today,
                        )
                        .await?;
                    }
                }
            }
        }

        Ok(recorded)
    }

    /// Spawns the periodic snapshot loop for the process's lifetime.
    pub fn spawn_periodic(pool: SqlitePool, interval: Duration) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                match Self::snapshot(&pool).await {
                    Ok(count) => tracing::debug!(items = count, "recorded price snapshot"),
                    Err(e) => tracing::warn!(error = %e, "price snapshot failed"),
                }
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_catalog_group(
    pool: &SqlitePool,
    defindex: u32,
    quality: Quality,
    craftable: bool,
    group: &CraftableEntry,
    key_rate: KeyRate,
    now: i64,
    today: i64,
) -> AppResult<u32> {
    let mut recorded = 0u32;
    match group {
        CraftableEntry::Plain(entries) => {
            if let Some(entry) = entries.first() {
                if record_one(
                    pool, defindex, quality, None, craftable, entry, key_rate, now, today,
                )
                .await?
                {
                    recorded += 1;
                }
            }
        }
        CraftableEntry::ByEffect(by_effect) => {
            for (effect_str, entry) in by_effect {
                let Ok(effect_id) = effect_str.parse::<u32>() else {
                    continue;
                };
                if record_one(
                    pool,
                    defindex,
                    quality,
                    Some(effect_id),
                    craftable,
                    entry,
                    key_rate,
                    now,
                    today,
                )
                .await?
                {
                    recorded += 1;
                }
            }
        }
    }
    Ok(recorded)
}

#[allow(clippy::too_many_arguments)]
async fn record_one(
    pool: &SqlitePool,
    defindex: u32,
    quality: Quality,
    effect_id: Option<u32>,
    craftable: bool,
    entry: &PriceEntry,
    key_rate: KeyRate,
    now: i64,
    today: i64,
) -> AppResult<bool> {
    let Some(value) = entry.value else {
        return Ok(false);
    };
    let ref_value = match entry.currency.as_deref() {
        Some("keys") => Currency::new(value, 0.0).value_in_ref(key_rate),
        _ => value,
    };

    let key = ItemKey {
        defindex,
        quality,
        effect_id,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable,
    };
    let Some(item_id) = ItemsRepo::find_id_by_key(pool, &key).await? else {
        return Ok(false);
    };

    PricePointsRepo::insert(
        pool,
        &InsertPricePoint {
            item_id,
            ts: now,
            source: "schema",
            best_buy_keys: None,
            best_buy_ref: None,
            best_sell_keys: None,
            best_sell_ref: Some(ref_value),
            buy_count: None,
            sell_count: None,
            key_rate_ref: key_rate.ref_per_key(),
        },
    )
    .await?;
    PriceDailyRepo::recompute_day(pool, item_id, today).await?;
    Ok(true)
}

/// The catalog prices the Key against itself (in metal), which is the
/// simplest live source for the current key↔ref rate.
fn find_key_rate(catalog: &PriceCatalogResponse) -> Option<KeyRate> {
    for catalog_item in catalog.items.values() {
        if !catalog_item
            .defindex
            .contains(&(MANN_CO_KEY_DEFINDEX as i64))
        {
            continue;
        }
        let quality_prices = catalog_item
            .prices
            .get(&(Quality::Unique as u8).to_string())?;
        let tradable = quality_prices.tradable.as_ref()?;
        let craftable = tradable.craftable.as_ref()?;
        let CraftableEntry::Plain(entries) = craftable else {
            continue;
        };
        let entry = entries.first()?;
        let value = entry.value?;
        if entry.currency.as_deref() == Some("keys") {
            continue;
        }
        return KeyRate::new(value).ok();
    }
    None
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
#[path = "history_recorder_tests.rs"]
mod tests;
