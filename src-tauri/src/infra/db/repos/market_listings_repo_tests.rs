use super::*;
use crate::infra::db;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-market-listings-repo-test-{}-{}",
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

fn row<'a>(
    listing_id: &'a str,
    defindex: i64,
    quality: i64,
    effect_id: Option<i64>,
    intent: &'a str,
    price: f64,
) -> UpsertMarketListing<'a> {
    UpsertMarketListing {
        listing_id,
        defindex,
        quality,
        effect_id,
        killstreak_tier: 0,
        australium: false,
        festivized: false,
        craftable: true,
        intent,
        price_ref: price,
        steam_id: "sid0",
        steam_name: Some("Trader"),
        updated_at: 1000,
    }
}

#[tokio::test]
async fn upsert_inserts_new_listing() {
    let (pool, dir) = test_pool().await;

    MarketListingsRepo::upsert(&pool, &row("l1", 5021, 6, None, "sell", 60.0))
        .await
        .unwrap();

    let rows = MarketListingsRepo::list_for_defindexes(&pool, &[5021], 6, None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].listing_id, "l1");
    assert_eq!(rows[0].defindex, 5021);
    assert_eq!(rows[0].quality, 6);
    assert_eq!(rows[0].effect_id, None);
    assert_eq!(rows[0].price_ref, 60.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn upsert_updates_existing_listing() {
    let (pool, dir) = test_pool().await;

    MarketListingsRepo::upsert(&pool, &row("l1", 5021, 6, None, "sell", 60.0))
        .await
        .unwrap();
    MarketListingsRepo::upsert(&pool, &row("l1", 5021, 6, None, "sell", 65.0))
        .await
        .unwrap();

    let rows = MarketListingsRepo::list_for_defindexes(&pool, &[5021], 6, None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].price_ref, 65.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn delete_removes_listing() {
    let (pool, dir) = test_pool().await;

    MarketListingsRepo::upsert(&pool, &row("l1", 5021, 6, None, "sell", 60.0))
        .await
        .unwrap();
    MarketListingsRepo::delete(&pool, "l1").await.unwrap();

    let rows = MarketListingsRepo::list_for_defindexes(&pool, &[5021], 6, None)
        .await
        .unwrap();
    assert!(rows.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_for_defindexes_filters_by_quality() {
    let (pool, dir) = test_pool().await;

    MarketListingsRepo::upsert(&pool, &row("l1", 5021, 6, None, "sell", 60.0))
        .await
        .unwrap();
    MarketListingsRepo::upsert(&pool, &row("l2", 5021, 11, None, "sell", 90.0))
        .await
        .unwrap();

    let rows = MarketListingsRepo::list_for_defindexes(&pool, &[5021], 6, None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].listing_id, "l1");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_for_defindexes_filters_by_effect_id_when_specified() {
    let (pool, dir) = test_pool().await;

    MarketListingsRepo::upsert(&pool, &row("l1", 200, 5, Some(701), "sell", 500.0))
        .await
        .unwrap();
    MarketListingsRepo::upsert(&pool, &row("l2", 200, 5, Some(13), "sell", 300.0))
        .await
        .unwrap();

    let rows = MarketListingsRepo::list_for_defindexes(&pool, &[200], 5, Some(701))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].listing_id, "l1");

    let all_effects = MarketListingsRepo::list_for_defindexes(&pool, &[200], 5, None)
        .await
        .unwrap();
    assert_eq!(all_effects.len(), 2);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_for_defindexes_aggregates_across_multiple_defindexes() {
    let (pool, dir) = test_pool().await;

    MarketListingsRepo::upsert(&pool, &row("l1", 200, 6, None, "sell", 10.0))
        .await
        .unwrap();
    MarketListingsRepo::upsert(&pool, &row("l2", 201, 6, None, "sell", 12.0))
        .await
        .unwrap();

    let rows = MarketListingsRepo::list_for_defindexes(&pool, &[200, 201], 6, None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_for_defindexes_returns_empty_for_empty_slice() {
    let (pool, dir) = test_pool().await;
    let rows = MarketListingsRepo::list_for_defindexes(&pool, &[], 6, None)
        .await
        .unwrap();
    assert!(rows.is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

fn plain_key(defindex: u32) -> ItemKey {
    ItemKey {
        defindex,
        quality: crate::domain::item::Quality::Unique,
        effect_id: None,
        killstreak_tier: crate::domain::item::KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    }
}

#[tokio::test]
async fn list_for_item_key_matches_only_the_exact_permutation() {
    let (pool, dir) = test_pool().await;

    // Same defindex+quality, but a Professional Killstreak Australium —
    // list_for_defindexes' coarser filter would conflate these; the exact
    // key match must not.
    MarketListingsRepo::upsert(&pool, &row("l1", 593, 11, None, "sell", 20.0))
        .await
        .unwrap();
    MarketListingsRepo::upsert(
        &pool,
        &UpsertMarketListing {
            killstreak_tier: 3,
            australium: true,
            ..row("l2", 593, 11, None, "sell", 900.0)
        },
    )
    .await
    .unwrap();

    let base_key = ItemKey {
        quality: crate::domain::item::Quality::Strange,
        ..plain_key(593)
    };
    let rows = MarketListingsRepo::list_for_item_key(&pool, &base_key)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].listing_id, "l1");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_for_item_key_treats_none_effect_id_as_null_safe() {
    let (pool, dir) = test_pool().await;

    MarketListingsRepo::upsert(&pool, &row("l1", 30469, 5, None, "sell", 100.0))
        .await
        .unwrap();
    MarketListingsRepo::upsert(&pool, &row("l2", 30469, 5, Some(13), "sell", 500.0))
        .await
        .unwrap();

    let key = ItemKey {
        quality: crate::domain::item::Quality::Unusual,
        ..plain_key(30469)
    };
    let rows = MarketListingsRepo::list_for_item_key(&pool, &key)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].listing_id, "l1");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_for_item_key_returns_empty_when_nothing_matches() {
    let (pool, dir) = test_pool().await;
    let rows = MarketListingsRepo::list_for_item_key(&pool, &plain_key(999_999))
        .await
        .unwrap();
    assert!(rows.is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
