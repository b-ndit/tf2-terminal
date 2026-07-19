//! Read-only lookup of another Steam account's public TF2 inventory, for
//! the minimal "propose a trade" flow (`commands::trades::get_public_inventory`) —
//! picking items to ask for once a partner SteamID64 is entered. Distinct
//! from `trade_analysis_engine`'s partner-item resolution: this has no
//! trade offer to value against, just a name/icon lookup per item, so it
//! skips `item_valuation::value_item_key`'s live pricing entirely rather
//! than pricing a stranger's entire inventory up front.

use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;

use crate::domain::steam_id::SteamId64;
use crate::error::AppResult;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::steam::inventory::{SteamInventoryClient, TF2Item};
use crate::infra::steam::SteamApiClient;

#[derive(Debug, Clone, Serialize, Type)]
pub struct PartnerItemView {
    pub asset_id: String,
    pub name: String,
    pub quality: u8,
    pub effect_id: Option<u32>,
    pub killstreak_tier: u8,
    pub australium: bool,
    pub festivized: bool,
    pub image_url: Option<String>,
}

/// Fetches `partner_steam_id`'s public TF2 inventory live (no local cache —
/// unlike `trade_analysis_engine`, there's no pending offer to justify one)
/// and resolves each item's display name/icon against our own catalog.
pub async fn get_public_inventory(
    pool: &SqlitePool,
    api: &SteamApiClient,
    api_key: String,
    partner_steam_id: SteamId64,
) -> AppResult<Vec<PartnerItemView>> {
    let client = SteamInventoryClient::new(api, api_key);
    let items = client.fetch_items(partner_steam_id).await?;
    resolve_views(pool, items).await
}

/// Resolves already-fetched `TF2Item`s against our own catalog — split out
/// from `get_public_inventory` so this (the actual logic) is testable
/// against a seeded pool without a live Steam call. Items the catalog has
/// never seen (schema never synced, never appeared in any inventory/
/// listing before) fall back to "Unknown Item {defindex}" with no icon —
/// same convention as everywhere else in the app.
async fn resolve_views(pool: &SqlitePool, items: Vec<TF2Item>) -> AppResult<Vec<PartnerItemView>> {
    let mut views = Vec::with_capacity(items.len());
    for tf2_item in items {
        let Ok(key) = tf2_item.item_key() else {
            continue;
        };

        let (name, image_url) = match ItemsRepo::find_id_by_key(pool, &key).await? {
            Some(id) => match ItemsRepo::find_by_id(pool, id).await? {
                Some(row) => (row.name, row.image_url),
                None => (format!("Unknown Item {}", key.defindex), None),
            },
            None => (format!("Unknown Item {}", key.defindex), None),
        };

        views.push(PartnerItemView {
            asset_id: tf2_item.id.to_string(),
            name,
            quality: key.quality as u8,
            effect_id: key.effect_id,
            killstreak_tier: key.killstreak_tier as u8,
            australium: key.australium,
            festivized: key.festivized,
            image_url,
        });
    }

    Ok(views)
}

#[cfg(test)]
#[path = "partner_inventory_service_tests.rs"]
mod tests;
