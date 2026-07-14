use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::items_repo::ItemsRepo;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-inventory-repo-test-{}-{}",
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

async fn seed_item(pool: &SqlitePool) -> i64 {
    let key = ItemKey {
        defindex: 5021,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    ItemsRepo::get_or_create(pool, &key, "Mann Co. Supply Crate Key")
        .await
        .unwrap()
}

#[tokio::test]
async fn upsert_inserts_new_asset() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    InventoryRepo::upsert(
        &pool,
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

    let existing = InventoryRepo::existing_for_steam_id(&pool, "sid0")
        .await
        .unwrap();
    assert_eq!(existing.len(), 1);
    assert_eq!(existing[0].asset_id, "111");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn upsert_preserves_acquired_ts_across_updates() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    let make_row = |ts: i64| UpsertInventoryItem {
        asset_id: "111",
        item_id,
        steam_id: "sid0",
        craft_number: None,
        paint_id: None,
        strange_count: None,
        tradable: true,
        marketable: None,
        last_seen_ts: ts,
        raw_json: "{}",
    };

    InventoryRepo::upsert(&pool, &make_row(1000)).await.unwrap();
    InventoryRepo::upsert(&pool, &make_row(2000)).await.unwrap();

    let acquired_ts: Option<i64> =
        sqlx::query_scalar("SELECT acquired_ts FROM inventory_items WHERE asset_id = '111'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let last_seen_ts: i64 =
        sqlx::query_scalar("SELECT last_seen_ts FROM inventory_items WHERE asset_id = '111'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(acquired_ts, Some(1000));
    assert_eq!(last_seen_ts, 2000);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn touch_last_seen_updates_only_that_column() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    InventoryRepo::upsert(
        &pool,
        &UpsertInventoryItem {
            asset_id: "111",
            item_id,
            steam_id: "sid",
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

    InventoryRepo::touch_last_seen(&pool, "111", 5000)
        .await
        .unwrap();

    let last_seen_ts: i64 =
        sqlx::query_scalar("SELECT last_seen_ts FROM inventory_items WHERE asset_id = '111'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last_seen_ts, 5000);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn remove_by_asset_ids_deletes_only_specified_rows() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    for asset_id in ["a", "b", "c"] {
        InventoryRepo::upsert(
            &pool,
            &UpsertInventoryItem {
                asset_id,
                item_id,
                steam_id: "sid",
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

    InventoryRepo::remove_by_asset_ids(&pool, &["a".to_string(), "c".to_string()])
        .await
        .unwrap();

    let remaining = InventoryRepo::existing_for_steam_id(&pool, "sid")
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].asset_id, "b");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn remove_by_asset_ids_is_a_noop_for_empty_slice() {
    let (pool, dir) = test_pool().await;
    InventoryRepo::remove_by_asset_ids(&pool, &[])
        .await
        .unwrap();
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_with_items_joins_item_metadata() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    InventoryRepo::upsert(
        &pool,
        &UpsertInventoryItem {
            asset_id: "111",
            item_id,
            steam_id: "sid",
            craft_number: Some(42),
            paint_id: None,
            strange_count: None,
            tradable: true,
            marketable: Some(false),
            last_seen_ts: 1000,
            raw_json: "{}",
        },
    )
    .await
    .unwrap();

    let views = InventoryRepo::list_with_items(&pool, "sid").await.unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "Mann Co. Supply Crate Key");
    assert_eq!(views[0].craft_number, Some(42));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn count_for_steam_id_counts_only_that_account() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    for (asset_id, steam_id) in [("a", "sid1"), ("b", "sid1"), ("c", "sid2")] {
        InventoryRepo::upsert(
            &pool,
            &UpsertInventoryItem {
                asset_id,
                item_id,
                steam_id,
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

    assert_eq!(
        InventoryRepo::count_for_steam_id(&pool, "sid1")
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        InventoryRepo::count_for_steam_id(&pool, "sid2")
            .await
            .unwrap(),
        1
    );

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_by_asset_ids_resolves_only_requested_ids_for_that_steam_id() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    for (asset_id, steam_id) in [("a", "sid1"), ("b", "sid1"), ("c", "sid2")] {
        InventoryRepo::upsert(
            &pool,
            &UpsertInventoryItem {
                asset_id,
                item_id,
                steam_id,
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

    let found = InventoryRepo::find_by_asset_ids(
        &pool,
        "sid1",
        &["a".to_string(), "c".to_string(), "missing".to_string()],
    )
    .await
    .unwrap();

    // "c" belongs to sid2 and "missing" doesn't exist — neither should
    // show up under sid1's lookup.
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].asset_id, "a");
    assert_eq!(found[0].item_id, item_id);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_by_asset_ids_returns_empty_for_empty_slice() {
    let (pool, dir) = test_pool().await;
    let found = InventoryRepo::find_by_asset_ids(&pool, "sid1", &[])
        .await
        .unwrap();
    assert!(found.is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
