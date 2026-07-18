use std::time::Duration;

use serde::Serialize;
use specta::Type;

use crate::app::AppState;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::error::{AppError, AppResult};
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::kv_cache_repo::KvCacheRepo;
use crate::infra::keychain::{keys, Keychain};
use crate::infra::steam::schema::{SchemaItem, SchemaOverview, SteamSchemaClient};

const CACHE_KEY_OVERVIEW: &str = "steam_schema:overview";
const CACHE_KEY_ITEMS: &str = "steam_schema:items";
const SCHEMA_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

// usize/i64 are forbidden by Specta for TS export (platform-width / 64-bit,
// silent precision loss risk); u32 comfortably covers any of these counts.
#[derive(Debug, Serialize, Type)]
pub struct SchemaSyncSummary {
    pub items_synced: u32,
    pub particles_cached: u32,
    pub qualities_cached: u32,
    pub from_cache: bool,
    /// Total row count in `items` after this sync — can exceed
    /// `items_synced` once inventory/market data starts adding permutations
    /// (Strange, Unusual+effect, ...) beyond the schema's base entries.
    pub items_in_db: u32,
}

/// Syncs Valve's TF2 item schema: fetches (or reuses a cached, still-fresh
/// copy of) the schema overview and full item catalog, then seeds the
/// `items` table with each base item at its native quality. Permutations
/// (Strange, Unusual+effect, Australium, ...) get added later as they're
/// actually encountered in inventories/listings.
pub async fn sync(state: &AppState) -> AppResult<SchemaSyncSummary> {
    if let Some(cached) = KvCacheRepo::get(&state.db, CACHE_KEY_ITEMS).await? {
        let items: Vec<SchemaItem> = serde_json::from_slice(&cached)
            .map_err(|e| AppError::Internal(format!("corrupt cached schema items: {e}")))?;
        let overview = load_cached_overview(state).await?;
        return Ok(SchemaSyncSummary {
            items_synced: items.len() as u32,
            particles_cached: overview.as_ref().map_or(0, |o| o.particles.len()) as u32,
            qualities_cached: overview.as_ref().map_or(0, |o| o.quality_names.len()) as u32,
            from_cache: true,
            items_in_db: ItemsRepo::count(&state.db).await? as u32,
        });
    }

    let api_key = Keychain::get(keys::STEAM_API_KEY)?
        .ok_or_else(|| AppError::Config("Steam API key not set".to_string()))?;
    let client = SteamSchemaClient::new(&state.steam_api, api_key);

    let overview = client.fetch_overview().await?;
    let items = client.fetch_all_items().await?;

    let overview_json = serde_json::to_vec(&overview)
        .map_err(|e| AppError::Internal(format!("failed to serialize schema overview: {e}")))?;
    let items_json = serde_json::to_vec(&items)
        .map_err(|e| AppError::Internal(format!("failed to serialize schema items: {e}")))?;

    KvCacheRepo::set(&state.db, CACHE_KEY_OVERVIEW, &overview_json, SCHEMA_TTL).await?;
    KvCacheRepo::set(&state.db, CACHE_KEY_ITEMS, &items_json, SCHEMA_TTL).await?;

    for item in &items {
        let quality = Quality::try_from(item.item_quality).unwrap_or(Quality::Unique);
        let key = ItemKey {
            defindex: item.defindex,
            quality,
            effect_id: None,
            killstreak_tier: KillstreakTier::None,
            australium: false,
            festivized: false,
            craftable: true,
        };
        let id = ItemsRepo::get_or_create(&state.db, &key, &item.item_name).await?;
        if let Some(image_url) = &item.image_url {
            ItemsRepo::set_image_url(&state.db, id, image_url).await?;
        }
        tracing::debug!(sku = %key.to_sku(), item_id = id, "synced schema item");
    }

    Ok(SchemaSyncSummary {
        items_synced: items.len() as u32,
        particles_cached: overview.particles.len() as u32,
        qualities_cached: overview.quality_names.len() as u32,
        from_cache: false,
        items_in_db: ItemsRepo::count(&state.db).await? as u32,
    })
}

async fn load_cached_overview(state: &AppState) -> AppResult<Option<SchemaOverview>> {
    let Some(cached) = KvCacheRepo::get(&state.db, CACHE_KEY_OVERVIEW).await? else {
        return Ok(None);
    };
    let overview: SchemaOverview = serde_json::from_slice(&cached)
        .map_err(|e| AppError::Internal(format!("corrupt cached schema overview: {e}")))?;
    Ok(Some(overview))
}
