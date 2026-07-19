//! Module 13: values a *hypothetical* trade the user assembles by hand (the
//! drag-drop builder) — the exact same valuation/rating pipeline Module 9
//! uses for real trade offers, just without a live Steam trade offer to
//! resolve items from. "Give" items are the user's own (resolved by asset
//! id from their synced inventory, same as Module 9's given side);
//! "receive" items are directly-specified hypothetical `ItemKey`s — no
//! Steam call needed for *either* side, unlike Module 9's partner-
//! inventory fetch, since a hypothetical item was never owned by anyone in
//! particular.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::SqlitePool;

use crate::domain::item::{ItemError, ItemKey, KillstreakTier, Quality};
use crate::domain::trade_rating::{rate_trade, TradeSide, ValuedItem};
use crate::error::AppResult;
use crate::infra::db::repos::inventory_repo::InventoryRepo;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::services::item_valuation::{value_item_key, ItemValuation};
use crate::services::trade_analysis_engine::TradeItemView;

/// Wire-safe mirror of `domain::item::ItemKey` (Specta-safe primitives
/// only), converted via `TryFrom`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub struct ItemKeyInput {
    pub defindex: u32,
    pub quality: u8,
    pub effect_id: Option<u32>,
    pub killstreak_tier: u8,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
}

impl TryFrom<ItemKeyInput> for ItemKey {
    type Error = ItemError;

    fn try_from(input: ItemKeyInput) -> Result<Self, Self::Error> {
        Ok(ItemKey {
            defindex: input.defindex,
            quality: Quality::try_from(input.quality)?,
            effect_id: input.effect_id,
            killstreak_tier: KillstreakTier::try_from(input.killstreak_tier)?,
            australium: input.australium,
            festivized: input.festivized,
            craftable: input.craftable,
        })
    }
}

/// Mirrors `trade_analysis_engine::AnalyzedTradeOffer` minus the
/// Steam-specific fields (trade offer id, partner, message, time) — there
/// is no real offer behind a simulated trade.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SimulatedTradeView {
    pub given_items: Vec<TradeItemView>,
    pub received_items: Vec<TradeItemView>,
    pub stars: u8,
    pub given_total_ref: f64,
    pub received_total_ref: f64,
    pub net_ref: f64,
    pub roi_pct: Option<f64>,
    pub risk: String,
    pub explanation: Vec<String>,
    pub counteroffer_additional_ref: Option<f64>,
}

struct SideValuation {
    trade_side: TradeSide,
    views: Vec<TradeItemView>,
}

fn unresolved() -> (ValuedItem, TradeItemView) {
    let name = "Unresolved Item".to_string();
    (
        ValuedItem {
            name: name.clone(),
            estimated_ref: None,
            quicksell_ref: None,
            liquidity: 0.0,
            demand: 0.0,
            spread_pct: None,
        },
        TradeItemView {
            name,
            estimated_ref: None,
            asset_id: None,
            quality: None,
            effect_id: None,
            killstreak_tier: None,
            australium: None,
            festivized: None,
            paint_id: None,
            craft_number: None,
            strange_count: None,
            image_url: None,
        },
    )
}

fn to_valued_item(v: ItemValuation) -> ValuedItem {
    ValuedItem {
        name: v.name,
        estimated_ref: v.estimated_ref,
        quicksell_ref: v.quicksell_ref,
        liquidity: v.liquidity,
        demand: v.demand,
        spread_pct: v.spread_pct,
    }
}

/// Resolves "give" items from the user's own synced inventory by asset id
/// — same cache-first approach as Module 9's given side (no live Steam
/// call; an asset not found in the cache is reported unresolved rather
/// than triggering a fresh sync).
async fn value_given_side(
    pool: &SqlitePool,
    steam_id: &str,
    asset_ids: &[String],
    now_ts: i64,
) -> AppResult<SideValuation> {
    let found = InventoryRepo::find_by_asset_ids(pool, steam_id, asset_ids).await?;
    let mut by_asset: HashMap<String, i64> = found
        .into_iter()
        .map(|row| (row.asset_id, row.item_id))
        .collect();

    let mut items = Vec::with_capacity(asset_ids.len());
    let mut views = Vec::with_capacity(asset_ids.len());

    for asset_id in asset_ids {
        let Some(item_id) = by_asset.remove(asset_id) else {
            let (item, view) = unresolved();
            items.push(item);
            views.push(view);
            continue;
        };
        let Some(row) = ItemsRepo::find_by_id(pool, item_id).await? else {
            let (item, view) = unresolved();
            items.push(item);
            views.push(view);
            continue;
        };
        let Ok(key) = row.key() else {
            let (item, view) = unresolved();
            items.push(item);
            views.push(view);
            continue;
        };

        let valuation = value_item_key(pool, &key, &row.name, now_ts).await?;
        views.push(TradeItemView {
            name: valuation.name.clone(),
            estimated_ref: valuation.estimated_ref,
            asset_id: Some(asset_id.clone()),
            quality: Some(key.quality as u8),
            effect_id: key.effect_id,
            killstreak_tier: Some(key.killstreak_tier as u8),
            australium: Some(key.australium),
            festivized: Some(key.festivized),
            paint_id: None,
            craft_number: None,
            strange_count: None,
            image_url: valuation.image_url.clone(),
        });
        items.push(to_valued_item(valuation));
    }

    Ok(SideValuation {
        trade_side: TradeSide { items },
        views,
    })
}

/// Resolves "receive" items directly from user-picked hypothetical
/// `ItemKey`s — no Steam call at all, unlike Module 9's partner-inventory
/// fetch.
async fn value_received_side(
    pool: &SqlitePool,
    keys: &[ItemKeyInput],
    now_ts: i64,
) -> AppResult<SideValuation> {
    let mut items = Vec::with_capacity(keys.len());
    let mut views = Vec::with_capacity(keys.len());

    for key_input in keys {
        let Ok(key) = ItemKey::try_from(*key_input) else {
            let (item, view) = unresolved();
            items.push(item);
            views.push(view);
            continue;
        };
        let fallback_name = format!("Item {}", key.defindex);
        let valuation = value_item_key(pool, &key, &fallback_name, now_ts).await?;
        views.push(TradeItemView {
            name: valuation.name.clone(),
            estimated_ref: valuation.estimated_ref,
            asset_id: None,
            quality: Some(key.quality as u8),
            effect_id: key.effect_id,
            killstreak_tier: Some(key.killstreak_tier as u8),
            australium: Some(key.australium),
            festivized: Some(key.festivized),
            paint_id: None,
            craft_number: None,
            strange_count: None,
            image_url: valuation.image_url.clone(),
        });
        items.push(to_valued_item(valuation));
    }

    Ok(SideValuation {
        trade_side: TradeSide { items },
        views,
    })
}

pub async fn simulate_trade(
    pool: &SqlitePool,
    steam_id: &str,
    given_asset_ids: &[String],
    received_keys: &[ItemKeyInput],
    now_ts: i64,
) -> AppResult<SimulatedTradeView> {
    let given = value_given_side(pool, steam_id, given_asset_ids, now_ts).await?;
    let received = value_received_side(pool, received_keys, now_ts).await?;

    let verdict = rate_trade(&given.trade_side, &received.trade_side);

    Ok(SimulatedTradeView {
        given_items: given.views,
        received_items: received.views,
        stars: verdict.stars,
        given_total_ref: verdict.given_total_ref,
        received_total_ref: verdict.received_total_ref,
        net_ref: verdict.net_ref,
        roi_pct: verdict.roi_pct,
        risk: verdict.risk.as_str().to_string(),
        explanation: verdict.explanation,
        counteroffer_additional_ref: verdict.counteroffer.map(|c| c.additional_ref_needed),
    })
}

#[cfg(test)]
#[path = "simulator_service_tests.rs"]
mod tests;
