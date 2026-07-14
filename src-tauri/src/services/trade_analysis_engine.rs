//! Orchestrates Module 9 (Trade Analyzer): fetches active received Steam
//! trade offers, resolves both sides to priced items, and rates each
//! offer via `domain::trade_rating`. Pull-based — the frontend polls
//! `get_active_trades` on an interval (TanStack Query `refetchInterval`),
//! same "recompute fresh on request, no persistence" shape as Module 7's
//! `market_analyzer_service` — rather than a backend background task
//! pushing Tauri events, which would duplicate work Module 10 (Alerts)
//! owns anyway.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;

use crate::app::AppState;
use crate::domain::currency::{Currency, KeyRate};
use crate::domain::steam_id::SteamId64;
use crate::domain::trade_rating::{rate_trade, TradeSide, ValuedItem};
use crate::error::{AppError, AppResult};
use crate::infra::db::repos::inventory_repo::InventoryRepo;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::kv_cache_repo::KvCacheRepo;
use crate::infra::db::repos::price_history_repo::PricePointsRepo;
use crate::infra::keychain::{keys, Keychain};
use crate::infra::steam::inventory::{SteamInventoryClient, TF2Item};
use crate::infra::steam::trade_offers::{SteamTradeOfferClient, TradeOfferAsset};
use crate::infra::steam::SteamApiClient;
use crate::services::item_valuation::{value_item_key, ItemValuation};

/// A pending offer gets re-analyzed on every poll (every ~20s from the
/// frontend), but the partner's inventory rarely changes that fast —
/// cache it briefly to avoid a live `GetPlayerItems` call per poll per
/// offer. Mirrors `market_data_service::PRICE_CACHE_TTL`'s reasoning.
const PARTNER_ITEMS_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const PARTNER_ITEMS_CACHE_KEY_PREFIX: &str = "steam_trade:partner_items:";

#[derive(Debug, Clone, Serialize, Type)]
pub struct TradeItemView {
    pub name: String,
    /// `None` when the item couldn't be resolved/priced at all — the
    /// frontend should render this distinctly from a legitimately
    /// zero-value item.
    pub estimated_ref: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct AnalyzedTradeOffer {
    pub trade_offer_id: String,
    pub partner_steam_id: String,
    pub message: String,
    pub time_created: f64,
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
    /// Keys/metal breakdown of `counteroffer_additional_ref`, for a more
    /// natural-reading suggested message — `None` alongside `None` when no
    /// live key rate has been observed yet (`docs/DESIGN.md` §2: normalize
    /// to ref internally, derive keys/metal for display only).
    pub counteroffer_additional_keys: Option<f64>,
    pub counteroffer_additional_metal_ref: Option<f64>,
}

/// Fetches, values, and rates every active offer the user has received.
/// Requires Steam login + a Steam API key, same preconditions as
/// `inventory_service::sync`.
pub async fn get_active_trades(state: &AppState) -> AppResult<Vec<AnalyzedTradeOffer>> {
    let my_steam_id = state
        .config
        .read()
        .await
        .steam_id
        .ok_or_else(|| AppError::Config("not logged in with Steam".to_string()))?;
    let api_key = Keychain::get(keys::STEAM_API_KEY)?
        .ok_or_else(|| AppError::Config("Steam API key not set".to_string()))?;

    let offers_client = SteamTradeOfferClient::new(&state.steam_api, api_key.clone());
    let offers = offers_client.fetch_active_received_offers().await?;
    if offers.is_empty() {
        return Ok(Vec::new());
    }

    let now_ts = now_unix();
    let key_rate = PricePointsRepo::latest_key_rate(&state.db)
        .await?
        .and_then(|r| KeyRate::new(r).ok());

    let my_steam_id_str = my_steam_id.to_string();
    let all_given_asset_ids: Vec<String> = offers
        .iter()
        .flat_map(|o| o.items_to_give.iter().map(|a| a.assetid.clone()))
        .collect();
    let my_item_ids: HashMap<String, i64> =
        InventoryRepo::find_by_asset_ids(&state.db, &my_steam_id_str, &all_given_asset_ids)
            .await?
            .into_iter()
            .map(|row| (row.asset_id, row.item_id))
            .collect();

    let mut results = Vec::with_capacity(offers.len());
    for offer in &offers {
        let partner_steam_id = SteamId64::from_account_id(offer.accountid_other);
        let partner_items =
            fetch_partner_items_cached(&state.db, &state.steam_api, &api_key, partner_steam_id)
                .await;

        let given = value_given_side(&state.db, &my_item_ids, &offer.items_to_give, now_ts).await?;
        let received = value_received_side(
            &state.db,
            partner_items.as_ref(),
            &offer.items_to_receive,
            now_ts,
        )
        .await?;

        let mut verdict = rate_trade(&given.trade_side, &received.trade_side);
        if partner_items.is_none() && !offer.items_to_receive.is_empty() {
            verdict.explanation.insert(
                0,
                "Partner inventory is private or unavailable — received items could not be valued"
                    .to_string(),
            );
        }

        let (
            counteroffer_additional_ref,
            counteroffer_additional_keys,
            counteroffer_additional_metal_ref,
        ) = match verdict.counteroffer {
            Some(c) => {
                let (keys, metal) = match key_rate {
                    Some(rate) => {
                        let currency = Currency::from_total_ref(c.additional_ref_needed, rate);
                        (Some(currency.keys), Some(currency.metal_ref))
                    }
                    None => (None, None),
                };
                (Some(c.additional_ref_needed), keys, metal)
            }
            None => (None, None, None),
        };

        results.push(AnalyzedTradeOffer {
            trade_offer_id: offer.tradeofferid.to_string(),
            partner_steam_id: partner_steam_id.to_string(),
            message: offer.message.clone(),
            time_created: offer.time_created as f64,
            given_items: given.views,
            received_items: received.views,
            stars: verdict.stars,
            given_total_ref: verdict.given_total_ref,
            received_total_ref: verdict.received_total_ref,
            net_ref: verdict.net_ref,
            roi_pct: verdict.roi_pct,
            risk: verdict.risk.as_str().to_string(),
            explanation: verdict.explanation,
            counteroffer_additional_ref,
            counteroffer_additional_keys,
            counteroffer_additional_metal_ref,
        });
    }

    Ok(results)
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
        },
    )
}

/// Values the user's own side of a trade against the already-synced
/// inventory cache (`inventory_service::sync`) — no live Steam call, since
/// items in an active outgoing offer are still present in `GetPlayerItems`
/// and syncing is the user's own explicit action (the "Sync Inventory"
/// button). An asset not found in the cache (never synced, or synced
/// since) is reported unpriced rather than triggering a fresh fetch.
async fn value_given_side(
    pool: &SqlitePool,
    my_item_ids: &HashMap<String, i64>,
    assets: &[TradeOfferAsset],
    now_ts: i64,
) -> AppResult<SideValuation> {
    let mut items = Vec::with_capacity(assets.len());
    let mut views = Vec::with_capacity(assets.len());

    for asset in assets {
        let Some(&item_id) = my_item_ids.get(&asset.assetid) else {
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
        });
        items.push(to_valued_item(valuation));
    }

    Ok(SideValuation {
        trade_side: TradeSide { items },
        views,
    })
}

/// Values the partner's side of a trade against a live (or briefly cached)
/// `GetPlayerItems` snapshot of their inventory. `partner_items` is `None`
/// when that fetch failed entirely (private inventory, API error) — every
/// item on this side is reported unpriced in that case, rather than
/// failing the whole offer's analysis.
async fn value_received_side(
    pool: &SqlitePool,
    partner_items: Option<&HashMap<String, TF2Item>>,
    assets: &[TradeOfferAsset],
    now_ts: i64,
) -> AppResult<SideValuation> {
    let mut items = Vec::with_capacity(assets.len());
    let mut views = Vec::with_capacity(assets.len());

    for asset in assets {
        let tf2_item = partner_items.and_then(|m| m.get(&asset.assetid));
        let Some(tf2_item) = tf2_item else {
            let (item, view) = unresolved();
            items.push(item);
            views.push(view);
            continue;
        };
        let Ok(key) = tf2_item.item_key() else {
            let (item, view) = unresolved();
            items.push(item);
            views.push(view);
            continue;
        };

        let fallback_name = format!("Unknown Item {}", tf2_item.defindex);
        let valuation = value_item_key(pool, &key, &fallback_name, now_ts).await?;
        views.push(TradeItemView {
            name: valuation.name.clone(),
            estimated_ref: valuation.estimated_ref,
        });
        items.push(to_valued_item(valuation));
    }

    Ok(SideValuation {
        trade_side: TradeSide { items },
        views,
    })
}

/// Projects the shared `item_valuation::ItemValuation` (Module 11) down to
/// exactly what `domain::trade_rating::ValuedItem` needs — the extra
/// fields (`history_days`, trend, ...) are Flip Finder's concern, not
/// Trade Analyzer's.
fn to_valued_item(valuation: ItemValuation) -> ValuedItem {
    ValuedItem {
        name: valuation.name,
        estimated_ref: valuation.estimated_ref,
        quicksell_ref: valuation.quicksell_ref,
        liquidity: valuation.liquidity,
        demand: valuation.demand,
        spread_pct: valuation.spread_pct,
    }
}

/// Fetches the partner's TF2 inventory (`IEconItems_440/GetPlayerItems`
/// works for any public SteamID64, not just the logged-in user's — same
/// endpoint `infra::steam::inventory` already uses), keyed by asset id for
/// O(1) lookup against a trade offer's `items_to_receive`. Cached briefly
/// (`PARTNER_ITEMS_CACHE_TTL`) since the same pending offer gets
/// re-analyzed every poll. `None` on any failure (private inventory, API
/// error) — callers degrade to "unpriced" rather than failing the offer.
async fn fetch_partner_items_cached(
    pool: &SqlitePool,
    api: &SteamApiClient,
    api_key: &str,
    partner_steam_id: SteamId64,
) -> Option<HashMap<String, TF2Item>> {
    let cache_key = format!("{PARTNER_ITEMS_CACHE_KEY_PREFIX}{partner_steam_id}");

    if let Ok(Some(cached)) = KvCacheRepo::get(pool, &cache_key).await {
        if let Ok(items) = serde_json::from_slice::<Vec<TF2Item>>(&cached) {
            return Some(index_by_asset_id(items));
        }
    }

    let client = SteamInventoryClient::new(api, api_key.to_string());
    match client.fetch_items(partner_steam_id).await {
        Ok(items) => {
            if let Ok(json) = serde_json::to_vec(&items) {
                let _ = KvCacheRepo::set(pool, &cache_key, &json, PARTNER_ITEMS_CACHE_TTL).await;
            }
            Some(index_by_asset_id(items))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                partner_steam_id = %partner_steam_id,
                "failed to fetch partner inventory for trade valuation"
            );
            None
        }
    }
}

fn index_by_asset_id(items: Vec<TF2Item>) -> HashMap<String, TF2Item> {
    items.into_iter().map(|i| (i.id.to_string(), i)).collect()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
#[path = "trade_analysis_engine_tests.rs"]
mod tests;
