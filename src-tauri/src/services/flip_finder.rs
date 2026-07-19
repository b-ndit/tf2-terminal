//! Module 11: scans every currently-active item (`market_listings`) for
//! flip opportunities. Pull-based, like Module 9's `TradeAnalysisEngine` —
//! a ranked list is naturally something the user refreshes/re-filters
//! rather than something needing instant push, and there's no external API
//! left to rate-limit around: since Module 5's websocket-first pivot, this
//! "scan" is a pure internal read over already-ingested `market_listings`/
//! `price_daily`, not a sweep of per-item API calls (`docs/DESIGN.md` §6's
//! "capped scan rate to respect the API" no longer applies).
//!
//! One valuation pass per active item (`item_valuation::value_item_key`)
//! covers all three named candidate criteria at once (`docs/DESIGN.md`
//! §6): items get tagged `is_watched` / `is_high_volume` / `is_mover` from
//! data that single pass already fetched, rather than three separate
//! gather-then-value phases.

use std::collections::HashSet;

use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;

use crate::domain::flip_finder::{score_flip, FlipCandidate};
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::error::AppResult;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::MarketListingsRepo;
use crate::infra::db::repos::watchlist_repo::WatchlistRepo;
use crate::services::item_valuation::value_item_key;

/// Listing depth (`buy_count + sell_count`) at/above which an item counts
/// as "high volume" — a documented heuristic (`docs/DESIGN.md` §6 names
/// the criterion but not a threshold).
const HIGH_VOLUME_DEPTH_THRESHOLD: i64 = 20;
/// A "recent mover": `|d1%|` or `|d7%|` (from `domain::trend`) at/above
/// these — also documented heuristics.
const D1_MOVER_THRESHOLD_PCT: f64 = 10.0;
const D7_MOVER_THRESHOLD_PCT: f64 = 20.0;

#[derive(Debug, Clone, Serialize, Type)]
pub struct FlipOpportunityView {
    pub item_name: String,
    /// Display-only — same catalog-level fields `ItemTile`/`ItemIcon`
    /// already key off of. Flip candidates are defindex+quality-level
    /// (`ItemKey`), not a specific owned asset, so there's no paint/craft
    /// number/strange count here the way there is for an inventory item.
    pub quality: u8,
    pub effect_id: Option<u32>,
    pub killstreak_tier: u8,
    pub image_url: Option<String>,
    pub buy_price_ref: f64,
    pub sell_price_ref: f64,
    pub quicksell_ref: Option<f64>,
    pub expected_profit_ref: f64,
    pub roi_pct: f64,
    pub confidence: f64,
    pub est_sale_time_hours: Option<f64>,
    pub is_watched: bool,
    pub is_high_volume: bool,
    pub is_mover: bool,
}

/// Scans every active item, values it once, and returns the ranked (by
/// `expected_profit_ref` descending) flip opportunities among watched/
/// high-volume/recent-mover candidates. Always requires a genuinely
/// positive ROI — "configurable filters" (`min_roi_pct`/`min_confidence`)
/// raise the bar further, they don't let unprofitable flips through.
pub async fn scan(
    pool: &SqlitePool,
    now_ts: i64,
    min_roi_pct: Option<f64>,
    min_confidence: Option<f64>,
) -> AppResult<Vec<FlipOpportunityView>> {
    let aggregates = MarketListingsRepo::aggregate_by_item(pool).await?;
    let watched_item_ids: HashSet<i64> = WatchlistRepo::list_item_ids(pool)
        .await?
        .into_iter()
        .collect();

    let mut opportunities = Vec::new();
    for agg in &aggregates {
        let Some(buy_price_ref) = agg.best_sell_ref else {
            continue; // nothing currently for sale -> nothing to buy
        };
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

        // Only items the schema/inventory/market data has already named
        // are candidates — same "no name, nothing to show" boundary as
        // Modules 9/10.
        let Some(item_id) = ItemsRepo::find_id_by_key(pool, &key).await? else {
            continue;
        };

        let fallback_name = format!("Item {}", key.defindex);
        let valuation = value_item_key(pool, &key, &fallback_name, now_ts).await?;

        let Some(sell_price_ref) = valuation.estimated_ref else {
            continue; // no resale target to compare against
        };

        let candidate = FlipCandidate {
            buy_price_ref,
            sell_price_ref,
            liquidity: valuation.liquidity,
            history_days: valuation.history_days,
            est_sale_time_hours: valuation.avg_sell_listing_age_hours,
        };
        let Some(opportunity) = score_flip(&candidate) else {
            continue;
        };
        if opportunity.roi_pct <= 0.0 {
            continue;
        }

        let is_watched = watched_item_ids.contains(&item_id);
        let is_high_volume = (agg.buy_count + agg.sell_count) >= HIGH_VOLUME_DEPTH_THRESHOLD;
        let is_mover = valuation
            .trend_d1_pct
            .is_some_and(|d| d.abs() >= D1_MOVER_THRESHOLD_PCT)
            || valuation
                .trend_d7_pct
                .is_some_and(|d| d.abs() >= D7_MOVER_THRESHOLD_PCT);
        if !(is_watched || is_high_volume || is_mover) {
            continue;
        }
        if min_roi_pct.is_some_and(|min| opportunity.roi_pct < min) {
            continue;
        }
        if min_confidence.is_some_and(|min| opportunity.confidence < min) {
            continue;
        }

        opportunities.push(FlipOpportunityView {
            item_name: valuation.name,
            quality: key.quality as u8,
            effect_id: key.effect_id,
            killstreak_tier: key.killstreak_tier as u8,
            image_url: valuation.image_url.clone(),
            buy_price_ref,
            sell_price_ref,
            quicksell_ref: valuation.quicksell_ref,
            expected_profit_ref: opportunity.expected_profit_ref,
            roi_pct: opportunity.roi_pct,
            confidence: opportunity.confidence,
            est_sale_time_hours: opportunity.est_sale_time_hours,
            is_watched,
            is_high_volume,
            is_mover,
        });
    }

    opportunities.sort_by(|a, b| {
        b.expected_profit_ref
            .partial_cmp(&a.expected_profit_ref)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(opportunities)
}

#[cfg(test)]
#[path = "flip_finder_tests.rs"]
mod tests;
