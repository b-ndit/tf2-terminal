use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::items_repo::ItemsRepo;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-watchlist-repo-test-{}-{}",
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

async fn seed_item(pool: &SqlitePool, defindex: u32, name: &str) -> i64 {
    let key = ItemKey {
        defindex,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    ItemsRepo::get_or_create(pool, &key, name).await.unwrap()
}

#[tokio::test]
async fn add_then_list_item_ids_returns_the_watched_item() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Mann Co. Supply Crate Key").await;

    WatchlistRepo::add(&pool, item_id, 1000).await.unwrap();

    let ids = WatchlistRepo::list_item_ids(&pool).await.unwrap();
    assert_eq!(ids, vec![item_id]);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn add_is_idempotent_for_the_same_item() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;

    WatchlistRepo::add(&pool, item_id, 1000).await.unwrap();
    WatchlistRepo::add(&pool, item_id, 2000).await.unwrap();

    let ids = WatchlistRepo::list_item_ids(&pool).await.unwrap();
    assert_eq!(ids.len(), 1);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn remove_deletes_the_watch() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 5021, "Key").await;
    WatchlistRepo::add(&pool, item_id, 1000).await.unwrap();

    WatchlistRepo::remove(&pool, item_id).await.unwrap();

    assert!(WatchlistRepo::list_item_ids(&pool)
        .await
        .unwrap()
        .is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_with_items_joins_display_name_newest_first() {
    let (pool, dir) = test_pool().await;
    let key_id = seed_item(&pool, 5021, "Mann Co. Supply Crate Key").await;
    let hat_id = seed_item(&pool, 45, "Team Captain").await;

    WatchlistRepo::add(&pool, key_id, 1000).await.unwrap();
    WatchlistRepo::add(&pool, hat_id, 2000).await.unwrap();

    let rows = WatchlistRepo::list_with_items(&pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].item_name, "Team Captain");
    assert_eq!(rows[0].added_ts, 2000);
    assert_eq!(rows[1].item_name, "Mann Co. Supply Crate Key");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_item_ids_is_empty_when_nothing_watched() {
    let (pool, dir) = test_pool().await;
    assert!(WatchlistRepo::list_item_ids(&pool)
        .await
        .unwrap()
        .is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
