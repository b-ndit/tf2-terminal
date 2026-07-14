use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::inventory_repo::{InventoryRepo, UpsertInventoryItem};
use crate::infra::db::repos::items_repo::ItemsRepo;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-item-meta-repo-test-{}-{}",
        std::process::id(),
        uniq_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    (pool, dir)
}

fn uniq_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Seeds an items row + an inventory_items row so item_meta's FK constraint
/// is satisfiable, returning the asset_id.
async fn seed_inventory_item(pool: &SqlitePool) -> String {
    let key = ItemKey {
        defindex: 5021,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    let item_id = ItemsRepo::get_or_create(pool, &key, "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    InventoryRepo::upsert(
        pool,
        &UpsertInventoryItem {
            asset_id: "111",
            item_id,
            steam_id: "sid0",
            craft_number: None,
            paint_id: None,
            strange_count: None,
            tradable: true,
            marketable: None,
            last_seen_ts: 1000,
            raw_json: "{}",
        },
    )
    .await
    .unwrap();
    "111".to_string()
}

#[tokio::test]
async fn get_returns_none_when_no_meta_set() {
    let (pool, dir) = test_pool().await;
    let asset_id = seed_inventory_item(&pool).await;

    assert_eq!(ItemMetaRepo::get(&pool, &asset_id).await.unwrap(), None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn set_favorite_creates_and_updates() {
    let (pool, dir) = test_pool().await;
    let asset_id = seed_inventory_item(&pool).await;

    ItemMetaRepo::set_favorite(&pool, &asset_id, true)
        .await
        .unwrap();
    let meta = ItemMetaRepo::get(&pool, &asset_id).await.unwrap().unwrap();
    assert!(meta.favorite);
    assert!(!meta.pinned);

    ItemMetaRepo::set_favorite(&pool, &asset_id, false)
        .await
        .unwrap();
    let meta = ItemMetaRepo::get(&pool, &asset_id).await.unwrap().unwrap();
    assert!(!meta.favorite);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn setters_do_not_clobber_other_fields() {
    let (pool, dir) = test_pool().await;
    let asset_id = seed_inventory_item(&pool).await;

    ItemMetaRepo::set_favorite(&pool, &asset_id, true)
        .await
        .unwrap();
    ItemMetaRepo::set_pinned(&pool, &asset_id, true)
        .await
        .unwrap();
    ItemMetaRepo::set_folder(&pool, &asset_id, Some("Unusuals"))
        .await
        .unwrap();
    ItemMetaRepo::set_note(&pool, &asset_id, Some("trade for keys"))
        .await
        .unwrap();
    ItemMetaRepo::set_custom_label(&pool, &asset_id, Some("My Hat"))
        .await
        .unwrap();

    let meta = ItemMetaRepo::get(&pool, &asset_id).await.unwrap().unwrap();
    assert!(meta.favorite);
    assert!(meta.pinned);
    assert_eq!(meta.folder.as_deref(), Some("Unusuals"));
    assert_eq!(meta.note.as_deref(), Some("trade for keys"));
    assert_eq!(meta.custom_label.as_deref(), Some("My Hat"));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn set_folder_none_clears_it() {
    let (pool, dir) = test_pool().await;
    let asset_id = seed_inventory_item(&pool).await;

    ItemMetaRepo::set_folder(&pool, &asset_id, Some("Unusuals"))
        .await
        .unwrap();
    ItemMetaRepo::set_folder(&pool, &asset_id, None)
        .await
        .unwrap();

    let meta = ItemMetaRepo::get(&pool, &asset_id).await.unwrap().unwrap();
    assert_eq!(meta.folder, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_many_keys_by_asset_id_for_steam_id() {
    let (pool, dir) = test_pool().await;
    let asset_id = seed_inventory_item(&pool).await;
    ItemMetaRepo::set_favorite(&pool, &asset_id, true)
        .await
        .unwrap();

    let many = ItemMetaRepo::get_many(&pool, "sid0").await.unwrap();
    assert_eq!(many.len(), 1);
    assert!(many[&asset_id].favorite);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn deleting_inventory_item_cascades_to_item_meta() {
    let (pool, dir) = test_pool().await;
    let asset_id = seed_inventory_item(&pool).await;
    ItemMetaRepo::set_favorite(&pool, &asset_id, true)
        .await
        .unwrap();

    InventoryRepo::remove_by_asset_ids(&pool, std::slice::from_ref(&asset_id))
        .await
        .unwrap();

    assert_eq!(ItemMetaRepo::get(&pool, &asset_id).await.unwrap(), None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
