use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use specta::Type;

use crate::app::AppState;
use crate::domain::steam_id::SteamId64;
use crate::error::{AppError, AppResult};
use crate::infra::db::repos::inventory_repo::{InventoryRepo, UpsertInventoryItem};
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::keychain::{keys, Keychain};
use crate::infra::steam::inventory::SteamInventoryClient;

// usize is platform-width and Specta forbids exporting it to TypeScript
// (silent precision loss risk on 64-bit); u32 comfortably covers any real
// backpack's item count.
#[derive(Debug, Serialize, Type)]
pub struct InventorySyncSummary {
    pub added: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub removed: u32,
    pub total: u32,
}

/// Fetches the live TF2 inventory for `steam_id` and diffs it against what's
/// cached: unchanged assets only get their freshness (`last_seen_ts`)
/// bumped, changed/new assets get a full upsert (including the SKU lookup),
/// and assets no longer present get removed. Trigger is manual for now
/// (IPC command) — interval/post-trade triggers land with the modules that
/// actually have those events (Module 9/12).
pub async fn sync(state: &AppState, steam_id: SteamId64) -> AppResult<InventorySyncSummary> {
    let api_key = Keychain::get(keys::STEAM_API_KEY)?
        .ok_or_else(|| AppError::Config("Steam API key not set".to_string()))?;
    let client = SteamInventoryClient::new(&state.steam_api, api_key);

    let steam_id_str = steam_id.to_string();
    let live_items = client.fetch_items(steam_id).await?;

    let existing = InventoryRepo::existing_for_steam_id(&state.db, &steam_id_str).await?;
    let mut existing_by_asset: HashMap<String, String> = existing
        .into_iter()
        .map(|e| (e.asset_id, e.raw_json))
        .collect();

    let live_asset_ids: HashSet<String> =
        live_items.iter().map(|item| item.id.to_string()).collect();
    let removed_ids: Vec<String> = existing_by_asset
        .keys()
        .filter(|asset_id| !live_asset_ids.contains(*asset_id))
        .cloned()
        .collect();
    InventoryRepo::remove_by_asset_ids(&state.db, &removed_ids).await?;

    let now = now_unix();
    let mut added = 0;
    let mut updated = 0;
    let mut unchanged = 0;

    for item in &live_items {
        let asset_id = item.id.to_string();
        let raw_json = serde_json::to_string(item)
            .map_err(|e| AppError::Internal(format!("failed to serialize inventory item: {e}")))?;

        let previous_raw = existing_by_asset.remove(&asset_id);
        if previous_raw.as_deref() == Some(raw_json.as_str()) {
            InventoryRepo::touch_last_seen(&state.db, &asset_id, now).await?;
            unchanged += 1;
            continue;
        }

        let key = item
            .item_key()
            .map_err(|e| AppError::Internal(format!("asset {asset_id}: {e}")))?;
        let name = ItemsRepo::find_name_by_defindex(&state.db, item.defindex)
            .await?
            .unwrap_or_else(|| format!("Unknown Item {}", item.defindex));
        let item_id = ItemsRepo::get_or_create(&state.db, &key, &name).await?;

        InventoryRepo::upsert(
            &state.db,
            &UpsertInventoryItem {
                asset_id: &asset_id,
                item_id,
                steam_id: &steam_id_str,
                craft_number: item.craft_number(),
                paint_id: item.paint_rgb(),
                strange_count: item.strange_count(),
                tradable: !item.flag_cannot_trade,
                marketable: None,
                last_seen_ts: now,
                raw_json: &raw_json,
            },
        )
        .await?;

        if previous_raw.is_some() {
            updated += 1;
        } else {
            added += 1;
        }
    }

    Ok(InventorySyncSummary {
        added,
        updated,
        unchanged,
        removed: removed_ids.len() as u32,
        total: live_items.len() as u32,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}
