use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::inventory_repo::{InventoryRepo, UpsertInventoryItem};
use crate::infra::db::repos::items_repo::ItemsRepo;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-tags-repo-test-{}-{}",
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

async fn seed_inventory_item(pool: &SqlitePool, asset_id: &str) {
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
            asset_id,
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
}

#[tokio::test]
async fn create_then_list() {
    let (pool, dir) = test_pool().await;

    TagsRepo::create(&pool, "trade-bait", "#ff0000")
        .await
        .unwrap();
    let tags = TagsRepo::list(&pool).await.unwrap();

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "trade-bait");
    assert_eq!(tags[0].color, "#ff0000");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn create_is_idempotent_by_name_and_updates_color() {
    let (pool, dir) = test_pool().await;

    let id1 = TagsRepo::create(&pool, "trade-bait", "#ff0000")
        .await
        .unwrap();
    let id2 = TagsRepo::create(&pool, "trade-bait", "#00ff00")
        .await
        .unwrap();

    assert_eq!(id1, id2);
    let tags = TagsRepo::list(&pool).await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].color, "#00ff00");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn delete_removes_tag() {
    let (pool, dir) = test_pool().await;

    let id = TagsRepo::create(&pool, "trade-bait", "#ff0000")
        .await
        .unwrap();
    TagsRepo::delete(&pool, id).await.unwrap();

    assert_eq!(TagsRepo::list(&pool).await.unwrap().len(), 0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn add_and_remove_from_item() {
    let (pool, dir) = test_pool().await;
    seed_inventory_item(&pool, "111").await;
    let tag_id = TagsRepo::create(&pool, "unusual", "#8650AC").await.unwrap();

    TagsRepo::add_to_item(&pool, "111", tag_id).await.unwrap();
    let by_asset = TagsRepo::get_many_for_steam_id(&pool, "sid0")
        .await
        .unwrap();
    assert_eq!(by_asset["111"].len(), 1);
    assert_eq!(by_asset["111"][0].name, "unusual");

    TagsRepo::remove_from_item(&pool, "111", tag_id)
        .await
        .unwrap();
    let by_asset = TagsRepo::get_many_for_steam_id(&pool, "sid0")
        .await
        .unwrap();
    assert!(!by_asset.contains_key("111"));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn add_to_item_is_idempotent() {
    let (pool, dir) = test_pool().await;
    seed_inventory_item(&pool, "111").await;
    let tag_id = TagsRepo::create(&pool, "unusual", "#8650AC").await.unwrap();

    TagsRepo::add_to_item(&pool, "111", tag_id).await.unwrap();
    TagsRepo::add_to_item(&pool, "111", tag_id).await.unwrap();

    let by_asset = TagsRepo::get_many_for_steam_id(&pool, "sid0")
        .await
        .unwrap();
    assert_eq!(by_asset["111"].len(), 1);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn deleting_tag_cascades_to_item_tags() {
    let (pool, dir) = test_pool().await;
    seed_inventory_item(&pool, "111").await;
    let tag_id = TagsRepo::create(&pool, "unusual", "#8650AC").await.unwrap();
    TagsRepo::add_to_item(&pool, "111", tag_id).await.unwrap();

    TagsRepo::delete(&pool, tag_id).await.unwrap();

    let by_asset = TagsRepo::get_many_for_steam_id(&pool, "sid0")
        .await
        .unwrap();
    assert!(!by_asset.contains_key("111"));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_many_for_steam_id_groups_multiple_tags_per_asset() {
    let (pool, dir) = test_pool().await;
    seed_inventory_item(&pool, "111").await;
    let tag_a = TagsRepo::create(&pool, "a", "#111111").await.unwrap();
    let tag_b = TagsRepo::create(&pool, "b", "#222222").await.unwrap();

    TagsRepo::add_to_item(&pool, "111", tag_a).await.unwrap();
    TagsRepo::add_to_item(&pool, "111", tag_b).await.unwrap();

    let by_asset = TagsRepo::get_many_for_steam_id(&pool, "sid0")
        .await
        .unwrap();
    assert_eq!(by_asset["111"].len(), 2);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
