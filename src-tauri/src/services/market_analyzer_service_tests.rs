use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::UpsertMarketListing;
use crate::infra::db::repos::price_history_repo::{
    InsertPricePoint, PriceDailyRepo, PricePointsRepo,
};

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-market-analyzer-test-{}-{}",
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

async fn seed_item(pool: &SqlitePool, defindex: u32, quality: Quality, name: &str) {
    let key = ItemKey {
        defindex,
        quality,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    ItemsRepo::get_or_create(pool, &key, name).await.unwrap();
}

struct SeedListing<'a> {
    listing_id: &'a str,
    defindex: i64,
    quality: i64,
    effect_id: Option<i64>,
    intent: &'a str,
    price_ref: f64,
    updated_at: i64,
}

async fn seed_listing(pool: &SqlitePool, seed: SeedListing<'_>) {
    MarketListingsRepo::upsert(
        pool,
        &UpsertMarketListing {
            listing_id: seed.listing_id,
            defindex: seed.defindex,
            quality: seed.quality,
            effect_id: seed.effect_id,
            killstreak_tier: 0,
            australium: false,
            festivized: false,
            craftable: true,
            intent: seed.intent,
            price_ref: seed.price_ref,
            steam_id: "sid0",
            steam_name: Some("Trader"),
            updated_at: seed.updated_at,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn analyze_rejects_malformed_url() {
    let (pool, dir) = test_pool().await;
    let result = analyze_classified_url(&pool, "not a url", 1_000_000_000).await;
    assert!(matches!(result, Err(AppError::InvalidInput(_))));
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn analyze_rejects_unknown_item() {
    let (pool, dir) = test_pool().await;
    let url = "https://backpack.tf/classifieds?item=Nonexistent&quality=6";
    let result = analyze_classified_url(&pool, url, 1_000_000_000).await;
    assert!(matches!(result, Err(AppError::InvalidInput(_))));
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn analyze_computes_spread_and_tables_from_seeded_listings() {
    let (pool, dir) = test_pool().await;
    let now = 1_000_000_000;

    seed_item(&pool, 45, Quality::Unique, "Scattergun").await;
    seed_listing(
        &pool,
        SeedListing {
            listing_id: "b1",
            defindex: 45,
            quality: 6,
            effect_id: None,
            intent: "buy",
            price_ref: 10.0,
            updated_at: now - 3600,
        },
    )
    .await;
    seed_listing(
        &pool,
        SeedListing {
            listing_id: "b2",
            defindex: 45,
            quality: 6,
            effect_id: None,
            intent: "buy",
            price_ref: 9.0,
            updated_at: now - 7200,
        },
    )
    .await;
    seed_listing(
        &pool,
        SeedListing {
            listing_id: "s1",
            defindex: 45,
            quality: 6,
            effect_id: None,
            intent: "sell",
            price_ref: 12.0,
            updated_at: now - 1800,
        },
    )
    .await;

    let url = "https://backpack.tf/classifieds?item=Scattergun&quality=6";
    let analytics = analyze_classified_url(&pool, url, now).await.unwrap();

    assert_eq!(analytics.item_name, "Scattergun");
    assert_eq!(analytics.quality, 6);
    assert_eq!(analytics.buy_listings.len(), 2);
    assert_eq!(analytics.sell_listings.len(), 1);
    assert_eq!(analytics.spread_abs_ref, Some(2.0)); // 12 - 10
    assert!(analytics.estimated_quicksell_ref.is_some());
    assert!(analytics.liquidity_score > 0.0);
    assert!(analytics.demand_score > 0.0);

    // The buy listing seeded 1 hour ago should report ~1.0 age_hours.
    let newest_buy = analytics
        .buy_listings
        .iter()
        .find(|l| l.price_ref == 10.0)
        .unwrap();
    assert!((newest_buy.age_hours - 1.0).abs() < 0.01);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn analyze_filters_unusual_listings_by_effect_id() {
    let (pool, dir) = test_pool().await;
    let now = 1_000_000_000;

    seed_item(&pool, 200, Quality::Unusual, "Team Captain").await;
    seed_listing(
        &pool,
        SeedListing {
            listing_id: "u1",
            defindex: 200,
            quality: 5,
            effect_id: Some(701),
            intent: "sell",
            price_ref: 500.0,
            updated_at: now,
        },
    )
    .await;
    seed_listing(
        &pool,
        SeedListing {
            listing_id: "u2",
            defindex: 200,
            quality: 5,
            effect_id: Some(13),
            intent: "sell",
            price_ref: 300.0,
            updated_at: now,
        },
    )
    .await;

    let url = "https://backpack.tf/classifieds?item=Team+Captain&quality=5&particle=701";
    let analytics = analyze_classified_url(&pool, url, now).await.unwrap();

    assert_eq!(analytics.sell_listings.len(), 1);
    assert_eq!(analytics.sell_listings[0].price_ref, 500.0);
    assert_eq!(analytics.effect_id, Some(701));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

async fn seed_daily_bar(pool: &SqlitePool, item_id: i64, day: i64, value_ref: f64) {
    PricePointsRepo::insert(
        pool,
        &InsertPricePoint {
            item_id,
            ts: day * 86_400,
            source: "snapshot",
            best_buy_keys: None,
            best_buy_ref: Some(value_ref),
            best_sell_keys: None,
            best_sell_ref: Some(value_ref),
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
async fn analyze_populates_trend_fields_from_price_daily_history() {
    let (pool, dir) = test_pool().await;
    let now = 1_000_000_000;
    let today = now / 86_400;

    let key = ItemKey {
        defindex: 45,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    let item_id = ItemsRepo::get_or_create(&pool, &key, "Scattergun")
        .await
        .unwrap();
    seed_daily_bar(&pool, item_id, today - 1, 10.0).await;
    seed_daily_bar(&pool, item_id, today, 11.0).await;

    let url = "https://backpack.tf/classifieds?item=Scattergun&quality=6";
    let analytics = analyze_classified_url(&pool, url, now).await.unwrap();

    assert!(analytics.trend_ma7_ref.is_some());
    let d1 = analytics.trend_d1_pct.unwrap();
    assert!((d1 - 10.0).abs() < 1e-6); // (11 - 10) / 10 * 100

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn analyze_leaves_trend_fields_none_without_history() {
    let (pool, dir) = test_pool().await;
    seed_item(&pool, 45, Quality::Unique, "Scattergun").await;

    let url = "https://backpack.tf/classifieds?item=Scattergun&quality=6";
    let analytics = analyze_classified_url(&pool, url, 1_000_000_000)
        .await
        .unwrap();

    assert_eq!(analytics.trend_ma7_ref, None);
    assert_eq!(analytics.trend_d1_pct, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_price_history_rejects_unknown_item() {
    let (pool, dir) = test_pool().await;
    let url = "https://backpack.tf/classifieds?item=Nonexistent&quality=6";
    let result = get_price_history(&pool, url).await;
    assert!(matches!(result, Err(AppError::InvalidInput(_))));
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_price_history_is_empty_without_recorded_bars() {
    let (pool, dir) = test_pool().await;
    seed_item(&pool, 45, Quality::Unique, "Scattergun").await;

    let url = "https://backpack.tf/classifieds?item=Scattergun&quality=6";
    let bars = get_price_history(&pool, url).await.unwrap();
    assert!(bars.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_price_history_returns_daily_bars_in_order() {
    let (pool, dir) = test_pool().await;
    let today = 1_000_000_000 / 86_400;

    let key = ItemKey {
        defindex: 45,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    let item_id = ItemsRepo::get_or_create(&pool, &key, "Scattergun")
        .await
        .unwrap();
    seed_daily_bar(&pool, item_id, today - 1, 10.0).await;
    seed_daily_bar(&pool, item_id, today, 12.0).await;

    let url = "https://backpack.tf/classifieds?item=Scattergun&quality=6";
    let bars = get_price_history(&pool, url).await.unwrap();

    assert_eq!(bars.len(), 2);
    assert!(bars[0].ts < bars[1].ts);
    assert_eq!(bars[1].close_ref, 12.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn analyze_handles_no_listings_gracefully() {
    let (pool, dir) = test_pool().await;
    seed_item(&pool, 45, Quality::Unique, "Scattergun").await;

    let url = "https://backpack.tf/classifieds?item=Scattergun&quality=6";
    let analytics = analyze_classified_url(&pool, url, 1_000_000_000)
        .await
        .unwrap();

    assert!(analytics.buy_listings.is_empty());
    assert!(analytics.sell_listings.is_empty());
    assert_eq!(analytics.spread_abs_ref, None);
    assert_eq!(analytics.estimated_quicksell_ref, None);
    assert_eq!(analytics.liquidity_score, 0.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_key_rate_ref_resolves_from_the_keys_own_listings() {
    let (pool, dir) = test_pool().await;
    let now = 1_000_000_000;

    seed_item(&pool, 5021, Quality::Unique, "Mann Co. Supply Crate Key").await;
    seed_listing(
        &pool,
        SeedListing {
            listing_id: "sell1",
            defindex: 5021,
            quality: 6,
            effect_id: None,
            intent: "sell",
            price_ref: 63.5,
            updated_at: now - 3600,
        },
    )
    .await;

    let rate = get_key_rate_ref(&pool, now).await.unwrap();
    assert!(rate > 0.0, "expected a positive key rate, got {rate}");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn get_key_rate_ref_is_zero_without_any_key_data() {
    let (pool, dir) = test_pool().await;
    let rate = get_key_rate_ref(&pool, 1_000_000_000).await.unwrap();
    assert_eq!(rate, 0.0);
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
