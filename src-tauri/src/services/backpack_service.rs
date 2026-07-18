use serde::Serialize;
use specta::Type;

use crate::app::AppState;
use crate::domain::steam_id::SteamId64;
use crate::error::AppResult;
use crate::infra::db::repos::inventory_repo::InventoryRepo;
use crate::infra::db::repos::item_meta_repo::{ItemMeta, ItemMetaRepo};
use crate::infra::db::repos::tags_repo::{Tag, TagsRepo};

/// One backpack entry as the UI wants it: inventory/item data plus the
/// user's own organization layer (`docs/DESIGN.md` §5).
///
/// Field types are deliberately narrower than the DB's native `i64` —
/// Specta forbids exporting 64-bit/platform-width integers to TypeScript to
/// avoid silent precision loss, and everything here (row ids, craft
/// numbers, paint RGB values) comfortably fits `i32`/`u32`/`u8` in practice.
/// Timestamps use `f64`, which represents any realistic unix-seconds value
/// exactly (53 bits of integer precision) without hitting that restriction.
#[derive(Debug, Serialize, Type)]
pub struct BackpackItem {
    pub asset_id: String,
    pub item_id: i32,
    pub name: String,
    pub quality: u8,
    pub effect_id: Option<u32>,
    pub killstreak_tier: u8,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
    pub craft_number: Option<i32>,
    pub paint_id: Option<i32>,
    pub strange_count: Option<i32>,
    pub tradable: bool,
    pub marketable: Option<bool>,
    pub acquired_ts: Option<f64>,
    pub last_seen_ts: f64,
    pub image_url: Option<String>,
    pub meta: ItemMeta,
    pub tags: Vec<Tag>,
}

pub async fn get_backpack(state: &AppState, steam_id: SteamId64) -> AppResult<Vec<BackpackItem>> {
    let steam_id_str = steam_id.to_string();

    let items = InventoryRepo::list_with_items(&state.db, &steam_id_str).await?;
    let mut meta_by_asset = ItemMetaRepo::get_many(&state.db, &steam_id_str).await?;
    let mut tags_by_asset = TagsRepo::get_many_for_steam_id(&state.db, &steam_id_str).await?;

    Ok(items
        .into_iter()
        .map(|inv| {
            let meta = meta_by_asset.remove(&inv.asset_id).unwrap_or_default();
            let tags = tags_by_asset.remove(&inv.asset_id).unwrap_or_default();
            BackpackItem {
                asset_id: inv.asset_id,
                item_id: inv.item_id as i32,
                name: inv.name,
                quality: inv.quality as u8,
                effect_id: inv.effect_id.map(|e| e as u32),
                killstreak_tier: inv.killstreak_tier as u8,
                australium: inv.australium,
                festivized: inv.festivized,
                craftable: inv.craftable,
                craft_number: inv.craft_number.map(|c| c as i32),
                paint_id: inv.paint_id.map(|p| p as i32),
                strange_count: inv.strange_count.map(|s| s as i32),
                tradable: inv.tradable,
                marketable: inv.marketable,
                acquired_ts: inv.acquired_ts.map(|t| t as f64),
                last_seen_ts: inv.last_seen_ts as f64,
                image_url: inv.image_url,
                meta,
                tags,
            }
        })
        .collect())
}
