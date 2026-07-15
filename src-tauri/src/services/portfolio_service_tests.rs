use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::inventory_repo::UpsertInventoryItem;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::{MarketListingsRepo, UpsertMarketListing};
use crate::infra::db::repos::price_history_repo::{
    InsertPricePoint, PriceDailyRepo, PricePointsRepo,
};

const STEAM_ID: &str = "76561198000000000";
const NOW_DAY: i64 = 20_000;
const NOW_TS: i64 = NOW_DAY * DAY_SECONDS;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-portfolio-service-test-{}-{}",
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

fn key(defindex: u32, quality: Quality) -> ItemKey {
    ItemKey {
        defindex,
        quality,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    }
}

async fn seed_item(pool: &SqlitePool, item_key: &ItemKey, name: &str) -> i64 {
    ItemsRepo::get_or_create(pool, item_key, name)
        .await
        .unwrap()
}

async fn seed_owned(pool: &SqlitePool, asset_id: &str, item_id: i64, steam_id: &str) {
    InventoryRepo::upsert(
        pool,
        &UpsertInventoryItem {
            asset_id,
            item_id,
            steam_id,
            craft_number: None,
            paint_id: None,
            strange_count: None,
            tradable: true,
            marketable: None,
            last_seen_ts: NOW_TS,
            raw_json: "{}",
        },
    )
    .await
    .unwrap();
}

async fn seed_listing(
    pool: &SqlitePool,
    listing_id: &str,
    item_key: &ItemKey,
    intent: &str,
    price: f64,
) {
    MarketListingsRepo::upsert(
        pool,
        &UpsertMarketListing {
            listing_id,
            defindex: item_key.defindex as i64,
            quality: item_key.quality as u8 as i64,
            effect_id: item_key.effect_id.map(|e| e as i64),
            killstreak_tier: item_key.killstreak_tier as u8 as i64,
            australium: item_key.australium,
            festivized: item_key.festivized,
            craftable: item_key.craftable,
            intent,
            price_ref: price,
            steam_id: "trader",
            steam_name: Some("Trader"),
            updated_at: NOW_TS,
        },
    )
    .await
    .unwrap();
}

async fn seed_key_market(pool: &SqlitePool, price: f64) {
    seed_listing(
        pool,
        "key-sell",
        &key(DEFINDEX_KEY as u32, Quality::Unique),
        "sell",
        price,
    )
    .await;
}

async fn seed_daily_bar(pool: &SqlitePool, item_id: i64, day: i64, close_ref: f64) {
    let ts = day * DAY_SECONDS;
    PricePointsRepo::insert(
        pool,
        &InsertPricePoint {
            item_id,
            ts,
            source: "snapshot",
            best_buy_keys: None,
            best_buy_ref: Some(close_ref),
            best_sell_keys: None,
            best_sell_ref: Some(close_ref),
            buy_count: Some(1),
            sell_count: Some(1),
            key_rate_ref: 60.0,
        },
    )
    .await
    .unwrap();
    PriceDailyRepo::recompute_day(pool, item_id, day)
        .await
        .unwrap();
}

#[tokio::test]
async fn snapshot_now_values_a_simple_inventory() {
    let (pool, dir) = test_pool().await;
    seed_key_market(&pool, 60.0).await;

    let hat_key = key(200, Quality::Unique);
    let hat_id = seed_item(&pool, &hat_key, "Some Hat").await;
    seed_listing(&pool, "hat-sell", &hat_key, "sell", 20.0).await;
    seed_owned(&pool, "a1", hat_id, STEAM_ID).await;

    let snapshot = snapshot_now(&pool, STEAM_ID, NOW_TS).await.unwrap();

    assert!((snapshot.total_ref - 20.0).abs() < 1e-9);
    assert_eq!(snapshot.item_count, 1);
    assert_eq!(snapshot.pure_keys, 0);
    assert_eq!(snapshot.pure_metal_ref, 0.0);
    assert_eq!(snapshot.unusual_count, 0);
    assert_eq!(snapshot.australium_count, 0);
    assert!((snapshot.total_keys - 20.0 / 60.0).abs() < 1e-9);

    let persisted = PortfolioSnapshotsRepo::latest(&pool, STEAM_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.ts, NOW_TS);
    assert!((persisted.total_ref - 20.0).abs() < 1e-9);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn snapshot_now_groups_identical_items_and_tallies_pure_currency() {
    let (pool, dir) = test_pool().await;
    seed_key_market(&pool, 60.0).await;

    let refined_id = seed_item(
        &pool,
        &key(DEFINDEX_REFINED as u32, Quality::Unique),
        "Refined Metal",
    )
    .await;
    let reclaimed_id = seed_item(
        &pool,
        &key(DEFINDEX_RECLAIMED as u32, Quality::Unique),
        "Reclaimed Metal",
    )
    .await;
    let scrap_id = seed_item(
        &pool,
        &key(DEFINDEX_SCRAP as u32, Quality::Unique),
        "Scrap Metal",
    )
    .await;

    for asset in ["r1", "r2", "r3"] {
        seed_owned(&pool, asset, refined_id, STEAM_ID).await;
    }
    seed_owned(&pool, "c1", reclaimed_id, STEAM_ID).await;
    for asset in ["s1", "s2"] {
        seed_owned(&pool, asset, scrap_id, STEAM_ID).await;
    }

    let snapshot = snapshot_now(&pool, STEAM_ID, NOW_TS).await.unwrap();

    let expected_metal = 3.0 + RECLAIMED_REF + 2.0 * SCRAP_REF;
    assert!((snapshot.pure_metal_ref - expected_metal).abs() < 1e-9);
    assert_eq!(snapshot.item_count, 6);
    assert!((snapshot.total_ref - expected_metal).abs() < 1e-9);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn snapshot_now_tallies_unusual_and_australium_counts() {
    let (pool, dir) = test_pool().await;
    seed_key_market(&pool, 60.0).await;

    let unusual_id = seed_item(&pool, &key(1000, Quality::Unusual), "Unusual Hat").await;
    let australium_key = ItemKey {
        australium: true,
        ..key(2000, Quality::Unique)
    };
    let australium_id = seed_item(&pool, &australium_key, "Australium Weapon").await;

    seed_owned(&pool, "u1", unusual_id, STEAM_ID).await;
    seed_owned(&pool, "au1", australium_id, STEAM_ID).await;

    let snapshot = snapshot_now(&pool, STEAM_ID, NOW_TS).await.unwrap();

    assert_eq!(snapshot.unusual_count, 1);
    assert_eq!(snapshot.australium_count, 1);
    assert_eq!(snapshot.item_count, 2);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn snapshot_now_counts_owned_keys_as_pure_keys_not_itemized_value() {
    let (pool, dir) = test_pool().await;
    seed_key_market(&pool, 60.0).await;
    let key_id = seed_item(
        &pool,
        &key(DEFINDEX_KEY as u32, Quality::Unique),
        "Mann Co. Supply Crate Key",
    )
    .await;

    seed_owned(&pool, "k1", key_id, STEAM_ID).await;
    seed_owned(&pool, "k2", key_id, STEAM_ID).await;

    let snapshot = snapshot_now(&pool, STEAM_ID, NOW_TS).await.unwrap();

    assert_eq!(snapshot.pure_keys, 2);
    assert!((snapshot.total_ref - 2.0 * 60.0).abs() < 1e-9);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_portfolio_history_returns_persisted_snapshots_in_order() {
    let (pool, dir) = test_pool().await;
    PortfolioSnapshotsRepo::insert(
        &pool,
        &InsertPortfolioSnapshot {
            ts: 1000,
            steam_id: STEAM_ID,
            total_ref: 100.0,
            total_keys: 1.5,
            pure_keys: Some(1),
            pure_metal_ref: Some(2.0),
            item_count: Some(10),
            unusual_count: Some(0),
            australium_count: Some(0),
        },
    )
    .await
    .unwrap();
    PortfolioSnapshotsRepo::insert(
        &pool,
        &InsertPortfolioSnapshot {
            ts: 2000,
            steam_id: STEAM_ID,
            total_ref: 150.0,
            total_keys: 2.0,
            pure_keys: Some(1),
            pure_metal_ref: Some(2.0),
            item_count: Some(11),
            unusual_count: Some(0),
            australium_count: Some(0),
        },
    )
    .await
    .unwrap();

    let history = get_portfolio_history(&pool, STEAM_ID, 0).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].ts, 1000.0);
    assert_eq!(history[1].total_ref, 150.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_pl_windows_computes_change_from_nearest_prior_snapshot() {
    let (pool, dir) = test_pool().await;
    let today = NOW_TS;
    let seven_days_ago = today - 7 * DAY_SECONDS;

    PortfolioSnapshotsRepo::insert(
        &pool,
        &InsertPortfolioSnapshot {
            ts: seven_days_ago,
            steam_id: STEAM_ID,
            total_ref: 100.0,
            total_keys: 1.5,
            pure_keys: None,
            pure_metal_ref: None,
            item_count: None,
            unusual_count: None,
            australium_count: None,
        },
    )
    .await
    .unwrap();
    PortfolioSnapshotsRepo::insert(
        &pool,
        &InsertPortfolioSnapshot {
            ts: today,
            steam_id: STEAM_ID,
            total_ref: 150.0,
            total_keys: 2.0,
            pure_keys: None,
            pure_metal_ref: None,
            item_count: None,
            unusual_count: None,
            australium_count: None,
        },
    )
    .await
    .unwrap();

    let windows = get_pl_windows(&pool, STEAM_ID, today).await.unwrap();
    let d7 = windows.d7.expect("a snapshot 7 days back exists");
    assert!((d7.abs_ref - 50.0).abs() < 1e-9);
    assert!((d7.pct - 50.0).abs() < 1e-9);
    // As-of lookup (same semantics as domain::trend): with no snapshot
    // closer to 1 day back than the 7-day-old one, d1 falls back to that
    // same snapshot and so equals d7. d30 has nothing before it at all
    // (the 7-day-old snapshot is *after* 30 days ago) -> None.
    assert_eq!(windows.d1, windows.d7);
    assert!(windows.d30.is_none());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_pl_windows_is_all_none_without_any_snapshot() {
    let (pool, dir) = test_pool().await;
    let windows = get_pl_windows(&pool, STEAM_ID, NOW_TS).await.unwrap();
    assert!(windows.d1.is_none());
    assert!(windows.d7.is_none());
    assert!(windows.d30.is_none());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_winners_losers_sorts_by_the_requested_windows_pct_change() {
    let (pool, dir) = test_pool().await;

    let gainer_id = seed_item(&pool, &key(300, Quality::Unique), "Gainer").await;
    seed_owned(&pool, "g1", gainer_id, STEAM_ID).await;
    seed_daily_bar(&pool, gainer_id, NOW_DAY - 1, 100.0).await;
    seed_daily_bar(&pool, gainer_id, NOW_DAY, 150.0).await; // +50%

    let loser_id = seed_item(&pool, &key(301, Quality::Unique), "Loser").await;
    seed_owned(&pool, "l1", loser_id, STEAM_ID).await;
    seed_daily_bar(&pool, loser_id, NOW_DAY - 1, 100.0).await;
    seed_daily_bar(&pool, loser_id, NOW_DAY, 50.0).await; // -50%

    let movers = get_winners_losers(&pool, STEAM_ID, 1, NOW_TS)
        .await
        .unwrap();

    assert_eq!(movers.len(), 2);
    assert_eq!(movers[0].item_name, "Gainer");
    assert!(movers[0].change_pct.unwrap() > 0.0);
    assert_eq!(movers[1].item_name, "Loser");
    assert!(movers[1].change_pct.unwrap() < 0.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_winners_losers_is_empty_for_an_empty_inventory() {
    let (pool, dir) = test_pool().await;
    let movers = get_winners_losers(&pool, STEAM_ID, 7, NOW_TS)
        .await
        .unwrap();
    assert!(movers.is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
