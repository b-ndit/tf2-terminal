use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::infra::db::repos::items_repo::{ItemSearchFilters, ItemsRepo};

#[derive(Debug, Clone, Serialize, Type)]
pub struct ItemSearchResult {
    pub item_id: i32,
    pub defindex: u32,
    pub name: String,
    pub quality: u8,
    pub effect_id: Option<u32>,
    pub killstreak_tier: u8,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
}

/// Faceted catalog search (Module 13) — powers the Simulator's item
/// picker. Returns nothing if every filter is left unset, rather than
/// dumping the whole catalog (`ItemsRepo::search`'s own guard).
#[tauri::command]
#[specta::specta]
pub async fn search_items(
    state: State<'_, AppState>,
    name: Option<String>,
    quality: Option<u8>,
    killstreak_tier: Option<u8>,
    australium: Option<bool>,
    craftable: Option<bool>,
    has_effect: Option<bool>,
) -> AppResult<Vec<ItemSearchResult>> {
    let filters = ItemSearchFilters {
        name: name.as_deref(),
        quality,
        killstreak_tier,
        australium,
        craftable,
        has_effect,
    };
    let rows = ItemsRepo::search(&state.db, &filters).await?;
    Ok(rows
        .into_iter()
        .map(|r| ItemSearchResult {
            item_id: r.id as i32,
            defindex: r.defindex as u32,
            name: r.name,
            quality: r.quality as u8,
            effect_id: r.effect_id.map(|e| e as u32),
            killstreak_tier: r.killstreak_tier as u8,
            australium: r.australium,
            festivized: r.festivized,
            craftable: r.craftable,
        })
        .collect())
}
