use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::UpsertMarketListing;
use crate::infra::db::repos::price_history_repo::{
    InsertPricePoint, PriceDailyRepo, PricePointsRepo,
};

const DAY_SECONDS: i64 = 86_400;
/// A fixed "today" so every test's price_daily bars land in predictable
/// windows relative to `now_ts`, independent of the real wall clock.
const NOW_DAY: i64 = 20_000;
const NOW_TS: i64 = NOW_DAY * DAY_SECONDS;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-flip-finder-test-{}-{}",
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

fn plain_key(defindex: u32) -> ItemKey {
    ItemKey {
        defindex,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    }
}

async fn seed_item(pool: &SqlitePool, defindex: u32, name: &str) -> i64 {
    ItemsRepo::get_or_create(pool, &plain_key(defindex), name)
        .await
        .unwrap()
}

async fn seed_listing(
    pool: &SqlitePool,
    listing_id: &str,
    defindex: u32,
    intent: &str,
    price: f64,
    updated_at: i64,
) {
    MarketListingsRepo::upsert(
        pool,
        &UpsertMarketListing {
            listing_id,
            defindex: defindex as i64,
            quality: Quality::Unique as u8 as i64,
            effect_id: None,
            killstreak_tier: 0,
            australium: false,
            festivized: false,
            craftable: true,
            intent,
            price_ref: price,
            steam_id: "trader",
            steam_name: Some("Trader"),
            updated_at,
        },
    )
    .await
    .unwrap();
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
async fn scan_returns_empty_when_nothing_is_listed() {
    let (pool, dir) = test_pool().await;
    let opportunities = scan(&pool, NOW_TS, None, None).await.unwrap();
    assert!(opportunities.is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_includes_a_watched_item_even_with_low_depth() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 100, "Watched Hat").await;
    seed_listing(&pool, "s1", 100, "sell", 50.0, NOW_TS - 3600).await;
    seed_daily_bar(&pool, item_id, NOW_DAY - 2, 100.0).await;
    WatchlistRepo::add(&pool, item_id, NOW_TS).await.unwrap();

    let opportunities = scan(&pool, NOW_TS, None, None).await.unwrap();

    assert_eq!(opportunities.len(), 1);
    assert_eq!(opportunities[0].item_name, "Watched Hat");
    assert!(opportunities[0].is_watched);
    assert!(!opportunities[0].is_high_volume);
    assert!(opportunities[0].expected_profit_ref > 0.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_includes_a_high_volume_item_by_listing_depth() {
    let (pool, dir) = test_pool().await;
    seed_item(&pool, 200, "Volume Hat").await;
    for i in 0..10 {
        seed_listing(
            &pool,
            &format!("buy{i}"),
            200,
            "buy",
            40.0 + i as f64,
            NOW_TS,
        )
        .await;
        seed_listing(
            &pool,
            &format!("sell{i}"),
            200,
            "sell",
            60.0 + i as f64,
            NOW_TS,
        )
        .await;
    }

    let opportunities = scan(&pool, NOW_TS, None, None).await.unwrap();

    assert_eq!(opportunities.len(), 1);
    assert!(opportunities[0].is_high_volume);
    assert!(!opportunities[0].is_watched);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_includes_a_recent_mover_by_trend() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 300, "Mover Hat").await;
    seed_listing(&pool, "s1", 300, "sell", 50.0, NOW_TS - 3600).await;
    // A 50% single-day drop -> well past the 10% d1 mover threshold.
    seed_daily_bar(&pool, item_id, NOW_DAY - 1, 100.0).await;
    seed_daily_bar(&pool, item_id, NOW_DAY, 50.0).await;

    let opportunities = scan(&pool, NOW_TS, None, None).await.unwrap();

    assert_eq!(opportunities.len(), 1);
    assert!(opportunities[0].is_mover);
    assert!(!opportunities[0].is_watched);
    assert!(!opportunities[0].is_high_volume);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_excludes_an_item_that_is_neither_watched_high_volume_nor_a_mover() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 400, "Boring Hat").await;
    seed_listing(&pool, "s1", 400, "sell", 50.0, NOW_TS - 3600).await;
    seed_daily_bar(&pool, item_id, NOW_DAY - 2, 51.0).await; // flat, not a mover

    let opportunities = scan(&pool, NOW_TS, None, None).await.unwrap();

    assert!(opportunities.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_excludes_unprofitable_flips_even_when_watched() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool, 500, "Overpriced Hat").await;
    // Current cheapest sell (100) is above the 30-day historical median
    // (50) -> a negative-ROI "flip".
    seed_listing(&pool, "s1", 500, "sell", 100.0, NOW_TS - 3600).await;
    seed_daily_bar(&pool, item_id, NOW_DAY - 2, 50.0).await;
    WatchlistRepo::add(&pool, item_id, NOW_TS).await.unwrap();

    let opportunities = scan(&pool, NOW_TS, None, None).await.unwrap();

    assert!(opportunities.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_applies_min_roi_pct_filter() {
    let (pool, dir) = test_pool().await;

    let low_roi_item = seed_item(&pool, 600, "Low ROI Hat").await;
    seed_listing(&pool, "low-sell", 600, "sell", 90.0, NOW_TS - 3600).await;
    seed_daily_bar(&pool, low_roi_item, NOW_DAY - 2, 100.0).await; // modest upside
    WatchlistRepo::add(&pool, low_roi_item, NOW_TS)
        .await
        .unwrap();

    let high_roi_item = seed_item(&pool, 601, "High ROI Hat").await;
    seed_listing(&pool, "high-sell", 601, "sell", 40.0, NOW_TS - 3600).await;
    seed_daily_bar(&pool, high_roi_item, NOW_DAY - 2, 200.0).await; // big upside
    WatchlistRepo::add(&pool, high_roi_item, NOW_TS)
        .await
        .unwrap();

    let unfiltered = scan(&pool, NOW_TS, None, None).await.unwrap();
    assert_eq!(unfiltered.len(), 2);
    let low = unfiltered
        .iter()
        .find(|o| o.item_name == "Low ROI Hat")
        .unwrap();
    let high = unfiltered
        .iter()
        .find(|o| o.item_name == "High ROI Hat")
        .unwrap();
    assert!(high.roi_pct > low.roi_pct);

    // A threshold strictly between the two should keep only the high one.
    let threshold = (low.roi_pct + high.roi_pct) / 2.0;
    let filtered = scan(&pool, NOW_TS, Some(threshold), None).await.unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].item_name, "High ROI Hat");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_applies_min_confidence_filter() {
    let (pool, dir) = test_pool().await;

    // Shallow history, thin depth -> low confidence.
    let shallow_item = seed_item(&pool, 700, "Shallow Hat").await;
    seed_listing(&pool, "shallow-sell", 700, "sell", 50.0, NOW_TS - 3600).await;
    seed_daily_bar(&pool, shallow_item, NOW_DAY - 2, 100.0).await;
    WatchlistRepo::add(&pool, shallow_item, NOW_TS)
        .await
        .unwrap();

    // Deep history (well past the confidence cap) plus real depth ->
    // higher confidence.
    let deep_item = seed_item(&pool, 701, "Deep Hat").await;
    for i in 0..15 {
        seed_listing(
            &pool,
            &format!("deep-buy{i}"),
            701,
            "buy",
            30.0 + i as f64,
            NOW_TS,
        )
        .await;
        seed_listing(
            &pool,
            &format!("deep-sell{i}"),
            701,
            "sell",
            45.0 + i as f64,
            NOW_TS,
        )
        .await;
    }
    for days_ago in 1..45 {
        seed_daily_bar(&pool, deep_item, NOW_DAY - days_ago, 100.0).await;
    }
    WatchlistRepo::add(&pool, deep_item, NOW_TS).await.unwrap();

    let unfiltered = scan(&pool, NOW_TS, None, None).await.unwrap();
    assert_eq!(unfiltered.len(), 2);
    let shallow = unfiltered
        .iter()
        .find(|o| o.item_name == "Shallow Hat")
        .unwrap();
    let deep = unfiltered
        .iter()
        .find(|o| o.item_name == "Deep Hat")
        .unwrap();
    assert!(deep.confidence > shallow.confidence);

    let threshold = (shallow.confidence + deep.confidence) / 2.0;
    let filtered = scan(&pool, NOW_TS, None, Some(threshold)).await.unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].item_name, "Deep Hat");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn scan_sorts_by_expected_profit_descending() {
    let (pool, dir) = test_pool().await;

    for (defindex, name, sell_price, history_price) in [
        (800_u32, "Small Profit Hat", 90.0, 100.0),
        (801, "Big Profit Hat", 40.0, 200.0),
        (802, "Medium Profit Hat", 70.0, 130.0),
    ] {
        let item_id = seed_item(&pool, defindex, name).await;
        seed_listing(
            &pool,
            &format!("s{defindex}"),
            defindex,
            "sell",
            sell_price,
            NOW_TS - 3600,
        )
        .await;
        seed_daily_bar(&pool, item_id, NOW_DAY - 2, history_price).await;
        WatchlistRepo::add(&pool, item_id, NOW_TS).await.unwrap();
    }

    let opportunities = scan(&pool, NOW_TS, None, None).await.unwrap();

    assert_eq!(opportunities.len(), 3);
    let names: Vec<&str> = opportunities.iter().map(|o| o.item_name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Big Profit Hat", "Medium Profit Hat", "Small Profit Hat"]
    );

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
