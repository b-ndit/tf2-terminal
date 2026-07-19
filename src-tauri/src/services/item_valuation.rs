//! Shared per-item valuation against live market data: resolve an exact
//! `ItemKey` to its current listings + `price_daily` history, then compute
//! spread/estimate/quicksell/liquidity/demand/trend. Extracted in Module 11
//! once Flip Finder became the third module needing this "resolve → value"
//! shape — Module 9's `trade_analysis_engine` had its own private copy
//! (moved here verbatim, enriched with the extra fields Flip Finder needs);
//! Module 7's `market_analyzer_service::analyze_query` resolves a *list* of
//! defindexes sharing a display name — a genuinely different shape — and
//! is left as its own thing.

use sqlx::SqlitePool;

use crate::domain::item::ItemKey;
use crate::domain::liquidity::{demand_score, liquidity_score};
use crate::domain::pricing::{self, Intent, Listing};
use crate::domain::trend::{self, PricePoint};
use crate::error::AppResult;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::MarketListingsRepo;
use crate::infra::db::repos::price_history_repo::PriceDailyRepo;

const DAY_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, PartialEq)]
pub struct ItemValuation {
    pub name: String,
    pub estimated_ref: Option<f64>,
    pub quicksell_ref: Option<f64>,
    pub liquidity: f64,
    pub demand: f64,
    pub spread_pct: Option<f64>,
    /// Count of `price_daily` bars on record for this item — Module 11's
    /// confidence-score input (more history, more confidence, with
    /// diminishing returns).
    pub history_days: u32,
    /// Mean age (hours) of this item's *current* sell listings — Module
    /// 11's `est_sale_time` proxy (a steady-state heuristic: how long
    /// listings typically sit before turning over). `None` if there are no
    /// current sell listings to measure.
    pub avg_sell_listing_age_hours: Option<f64>,
    pub trend_d1_pct: Option<f64>,
    pub trend_d7_pct: Option<f64>,
    /// The schema-sourced icon for this exact permutation, if the item
    /// schema has been synced — `None` otherwise (`schema_service::sync`).
    pub image_url: Option<String>,
}

/// Resolves `key` (creating the `items` row via `fallback_name` if this
/// exact permutation hasn't been seen before — same as
/// `inventory_service::sync`'s pattern) and values it against its current
/// listings and daily history.
pub async fn value_item_key(
    pool: &SqlitePool,
    key: &ItemKey,
    fallback_name: &str,
    now_ts: i64,
) -> AppResult<ItemValuation> {
    let name = ItemsRepo::find_name_by_defindex(pool, key.defindex)
        .await?
        .unwrap_or_else(|| fallback_name.to_string());
    let item_id = ItemsRepo::get_or_create(pool, key, &name).await?;
    let image_url = ItemsRepo::find_by_id(pool, item_id)
        .await?
        .and_then(|row| row.image_url);

    let history = daily_history(pool, item_id).await?;
    let rows = MarketListingsRepo::list_for_item_key(pool, key).await?;
    let listings: Vec<Listing> = rows
        .iter()
        .map(|r| Listing {
            intent: if r.intent == "buy" {
                Intent::Buy
            } else {
                Intent::Sell
            },
            price_ref: r.price_ref,
        })
        .collect();

    let spread = pricing::spread(&listings);
    let estimated_ref = pricing::estimate_sale_price(&listings, &history, now_ts);
    let quicksell_ref = pricing::estimate_quicksell(&listings);

    let ages: Vec<f64> = rows
        .iter()
        .map(|r| ((now_ts - r.updated_at).max(0) as f64) / 3600.0)
        .collect();
    let buy_count = rows.iter().filter(|r| r.intent == "buy").count() as u32;
    let liquidity = liquidity_score(rows.len() as u32, &ages, 0);
    let demand = demand_score(buy_count, 0.0, 0.0);

    let sell_ages: Vec<f64> = rows
        .iter()
        .zip(ages.iter())
        .filter(|(r, _)| r.intent == "sell")
        .map(|(_, age)| *age)
        .collect();
    let avg_sell_listing_age_hours = if sell_ages.is_empty() {
        None
    } else {
        Some(sell_ages.iter().sum::<f64>() / sell_ages.len() as f64)
    };

    let trend = trend::trend(&history, now_ts);

    Ok(ItemValuation {
        name,
        estimated_ref,
        quicksell_ref,
        liquidity,
        demand,
        spread_pct: spread.map(|s| s.pct),
        history_days: history.len() as u32,
        avg_sell_listing_age_hours,
        trend_d1_pct: trend.d1,
        trend_d7_pct: trend.d7,
        image_url,
    })
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

#[cfg(test)]
#[path = "item_valuation_tests.rs"]
mod tests;
