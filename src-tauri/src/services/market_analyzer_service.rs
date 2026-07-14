use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;

use crate::domain::classified_url::{self, ClassifiedQuery};
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::domain::liquidity::{demand_score, liquidity_score};
use crate::domain::pricing::{self, Intent, Listing};
use crate::domain::trend::{self, PricePoint};
use crate::error::{AppError, AppResult};
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::{MarketListingRow, MarketListingsRepo};
use crate::infra::db::repos::price_history_repo::PriceDailyRepo;

const DAY_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, Serialize, Type)]
pub struct ListingRow {
    pub listing_id: String,
    /// Which defindex this row came from — worth surfacing when a name
    /// resolves to several (class-specific reskins etc.), so the UI can
    /// distinguish them if it wants to.
    pub defindex: u32,
    pub steam_id: String,
    pub steam_name: Option<String>,
    pub price_ref: f64,
    pub age_hours: f64,
}

/// Current-moment analytics for one item (defindex(es) + quality + optional
/// unusual effect), computed from `market_listings` — our own accumulated
/// view of the live websocket feed (Module 5's snapshot-endpoint deviation
/// means there's no on-demand REST source for this).
///
/// `liquidity_score`/`demand_score` are computed with `volume_7d`,
/// `buy_growth_pct`, and `sale_velocity_per_day` all at their neutral/zero
/// defaults — we don't yet track completed trades over time, so these two
/// scores lean entirely on current depth and listing freshness.
///
/// The `trend_*` fields come from `price_daily` (Module 8's History
/// Recorder) and are `None` until enough daily bars have accumulated for
/// this exact item permutation.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ItemAnalytics {
    pub item_name: String,
    pub quality: u8,
    pub effect_id: Option<u32>,
    pub spread_abs_ref: Option<f64>,
    pub spread_pct: Option<f64>,
    pub liquidity_score: f64,
    pub demand_score: f64,
    pub estimated_sale_price_ref: Option<f64>,
    pub estimated_quicksell_ref: Option<f64>,
    pub buy_listings: Vec<ListingRow>,
    pub sell_listings: Vec<ListingRow>,
    pub trend_ma7_ref: Option<f64>,
    pub trend_ma30_ref: Option<f64>,
    pub trend_volatility_pct: Option<f64>,
    pub trend_d1_pct: Option<f64>,
    pub trend_d7_pct: Option<f64>,
    pub trend_d30_pct: Option<f64>,
    pub trend_d365_pct: Option<f64>,
}

/// One daily OHLC bar (`price_daily`), for a charting panel. `ts` is a unix
/// timestamp in seconds — Specta forbids exporting `i64` to TypeScript, and
/// `f64` holds this magnitude exactly (well under 2^53).
#[derive(Debug, Clone, Serialize, Type)]
pub struct PriceBar {
    pub ts: f64,
    pub open_ref: f64,
    pub high_ref: f64,
    pub low_ref: f64,
    pub close_ref: f64,
    pub samples: u32,
}

pub async fn analyze_classified_url(
    pool: &SqlitePool,
    url: &str,
    now_ts: i64,
) -> AppResult<ItemAnalytics> {
    let query = classified_url::parse_classified_url(url)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    analyze_query(pool, &query, now_ts).await
}

/// Resolves a classifieds URL to a concrete stored item, creating the
/// `items` row if this exact permutation hasn't been seen before. Reused
/// by `commands::alerts` (Module 10) so alert rules can target the same
/// items Market Analyzer already knows how to resolve, without
/// duplicating URL parsing.
pub async fn resolve_item_id_from_url(pool: &SqlitePool, url: &str) -> AppResult<(i64, String)> {
    let query = classified_url::parse_classified_url(url)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;

    let defindexes = ItemsRepo::find_defindexes_by_name(pool, &query.item_name).await?;
    if defindexes.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "unknown item '{}' — try syncing the item schema first",
            query.item_name
        )));
    }

    let item_key = resolve_item_key(defindexes[0] as u32, &query)?;
    let name = ItemsRepo::find_name_by_defindex(pool, defindexes[0] as u32)
        .await?
        .unwrap_or_else(|| query.item_name.clone());
    let item_id = ItemsRepo::get_or_create(pool, &item_key, &name).await?;

    Ok((item_id, name))
}

/// The classifieds URL's own permutation fields (killstreak/australium/
/// craftable) already resolve to an exact `ItemKey`, unlike Module 7's
/// `market_listings` lookup below (which only ever filtered on
/// quality+effect_id — left as-is here, out of scope for Module 8).
fn resolve_item_key(defindex: u32, query: &ClassifiedQuery) -> AppResult<ItemKey> {
    let quality =
        Quality::try_from(query.quality).map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let killstreak_tier = query
        .killstreak_tier
        .map(KillstreakTier::try_from)
        .transpose()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?
        .unwrap_or_default();
    Ok(ItemKey {
        defindex,
        quality,
        effect_id: query.particle,
        killstreak_tier,
        australium: query.australium.unwrap_or(false),
        // Classifieds URLs carry no festivized param.
        festivized: false,
        craftable: query.craftable.unwrap_or(true),
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

async fn analyze_query(
    pool: &SqlitePool,
    query: &ClassifiedQuery,
    now_ts: i64,
) -> AppResult<ItemAnalytics> {
    let defindexes = ItemsRepo::find_defindexes_by_name(pool, &query.item_name).await?;
    if defindexes.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "unknown item '{}' — try syncing the item schema first",
            query.item_name
        )));
    }

    let item_key = resolve_item_key(defindexes[0] as u32, query)?;
    let item_id = ItemsRepo::find_id_by_key(pool, &item_key).await?;
    let history = match item_id {
        Some(id) => daily_history(pool, id).await?,
        None => Vec::new(),
    };
    let trend = trend::trend(&history, now_ts);

    let effect_id = query.particle.map(|e| e as i64);
    let rows =
        MarketListingsRepo::list_for_defindexes(pool, &defindexes, query.quality as i64, effect_id)
            .await?;

    let (buy_rows, sell_rows): (Vec<&MarketListingRow>, Vec<&MarketListingRow>) =
        rows.iter().partition(|r| r.intent == "buy");

    let to_listing_row = |r: &&MarketListingRow| ListingRow {
        listing_id: r.listing_id.clone(),
        defindex: r.defindex as u32,
        steam_id: r.steam_id.clone(),
        steam_name: r.steam_name.clone(),
        price_ref: r.price_ref,
        age_hours: ((now_ts - r.updated_at).max(0) as f64) / 3600.0,
    };
    let buy_listings: Vec<ListingRow> = buy_rows.iter().map(to_listing_row).collect();
    let sell_listings: Vec<ListingRow> = sell_rows.iter().map(to_listing_row).collect();

    let pricing_listings: Vec<Listing> = rows
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

    let spread = pricing::spread(&pricing_listings);
    let estimated_sale_price_ref =
        pricing::estimate_sale_price(&pricing_listings, &history, now_ts);
    let estimated_quicksell_ref = pricing::estimate_quicksell(&pricing_listings);

    let all_ages_hours: Vec<f64> = buy_listings
        .iter()
        .chain(sell_listings.iter())
        .map(|l| l.age_hours)
        .collect();
    let depth = rows.len() as u32;
    let liquidity = liquidity_score(depth, &all_ages_hours, 0);
    let demand = demand_score(buy_listings.len() as u32, 0.0, 0.0);

    let item_name = ItemsRepo::find_name_by_defindex(pool, defindexes[0] as u32)
        .await?
        .unwrap_or_else(|| query.item_name.clone());

    Ok(ItemAnalytics {
        item_name,
        quality: query.quality,
        effect_id: query.particle,
        spread_abs_ref: spread.map(|s| s.abs_ref),
        spread_pct: spread.map(|s| s.pct),
        liquidity_score: liquidity,
        demand_score: demand,
        estimated_sale_price_ref,
        estimated_quicksell_ref,
        buy_listings,
        sell_listings,
        trend_ma7_ref: trend.ma7,
        trend_ma30_ref: trend.ma30,
        trend_volatility_pct: trend.volatility,
        trend_d1_pct: trend.d1,
        trend_d7_pct: trend.d7,
        trend_d30_pct: trend.d30,
        trend_d365_pct: trend.d365,
    })
}

/// Daily OHLC bars for `url`'s resolved item, for a charting panel. `Ok(&
/// [])` (not an error) if the item's schema is known but no price history
/// has accumulated for this exact permutation yet.
pub async fn get_price_history(pool: &SqlitePool, url: &str) -> AppResult<Vec<PriceBar>> {
    let query = classified_url::parse_classified_url(url)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;

    let defindexes = ItemsRepo::find_defindexes_by_name(pool, &query.item_name).await?;
    if defindexes.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "unknown item '{}' — try syncing the item schema first",
            query.item_name
        )));
    }

    let item_key = resolve_item_key(defindexes[0] as u32, &query)?;
    let Some(item_id) = ItemsRepo::find_id_by_key(pool, &item_key).await? else {
        return Ok(Vec::new());
    };

    let rows = PriceDailyRepo::history_since(pool, item_id, 0).await?;
    let bars = rows
        .into_iter()
        .filter_map(|r| {
            Some(PriceBar {
                ts: (r.day * DAY_SECONDS) as f64,
                open_ref: r.open_ref?,
                high_ref: r.high_ref?,
                low_ref: r.low_ref?,
                close_ref: r.close_ref?,
                samples: r.samples.unwrap_or(0) as u32,
            })
        })
        .collect();
    Ok(bars)
}

#[cfg(test)]
#[path = "market_analyzer_service_tests.rs"]
mod tests;
