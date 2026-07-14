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
        "tf2-terminal-item-valuation-test-{}-{}",
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

async fn seed_listing(
    pool: &SqlitePool,
    listing_id: &str,
    key: &ItemKey,
    intent: &str,
    price: f64,
    updated_at: i64,
) {
    MarketListingsRepo::upsert(
        pool,
        &UpsertMarketListing {
            listing_id,
            defindex: key.defindex as i64,
            quality: key.quality as u8 as i64,
            effect_id: key.effect_id.map(|e| e as i64),
            killstreak_tier: key.killstreak_tier as u8 as i64,
            australium: key.australium,
            festivized: key.festivized,
            craftable: key.craftable,
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

#[tokio::test]
async fn value_item_key_computes_price_liquidity_and_demand() {
    let (pool, dir) = test_pool().await;
    let key = plain_key(5021);
    ItemsRepo::get_or_create(&pool, &key, "Mann Co. Supply Crate Key")
        .await
        .unwrap();

    seed_listing(&pool, "l1", &key, "buy", 50.0, 1000).await;
    seed_listing(&pool, "l2", &key, "sell", 55.0, 1000).await;

    let valuation = value_item_key(&pool, &key, "fallback", 2000).await.unwrap();

    assert_eq!(valuation.name, "Mann Co. Supply Crate Key");
    assert!(valuation.estimated_ref.is_some());
    assert!(valuation.quicksell_ref.is_some());
    assert!(valuation.liquidity > 0.0);
    assert!(valuation.demand > 0.0);
    assert!(valuation.spread_pct.is_some());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn value_item_key_uses_fallback_name_and_is_unpriced_when_unknown() {
    let (pool, dir) = test_pool().await;
    let key = plain_key(999_999);

    let valuation = value_item_key(&pool, &key, "Unknown Item 999999", 1000)
        .await
        .unwrap();

    assert_eq!(valuation.name, "Unknown Item 999999");
    assert_eq!(valuation.estimated_ref, None);
    assert_eq!(valuation.quicksell_ref, None);
    assert_eq!(valuation.history_days, 0);
    assert_eq!(valuation.avg_sell_listing_age_hours, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn value_item_key_computes_avg_sell_listing_age_from_sell_side_only() {
    let (pool, dir) = test_pool().await;
    let key = plain_key(5021);
    ItemsRepo::get_or_create(&pool, &key, "Key").await.unwrap();

    // Sell listings 1h and 3h old; a buy listing 100h old that shouldn't
    // count toward the sell-side average.
    seed_listing(&pool, "sell1", &key, "sell", 50.0, 1000 - 3600).await;
    seed_listing(&pool, "sell2", &key, "sell", 52.0, 1000 - 3 * 3600).await;
    seed_listing(&pool, "buy1", &key, "buy", 40.0, 1000 - 100 * 3600).await;

    let valuation = value_item_key(&pool, &key, "fallback", 1000).await.unwrap();

    let avg = valuation.avg_sell_listing_age_hours.unwrap();
    assert!((avg - 2.0).abs() < 1e-9); // mean(1, 3) = 2

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn value_item_key_counts_history_days_and_computes_trend() {
    let (pool, dir) = test_pool().await;
    let key = plain_key(5021);
    let item_id = ItemsRepo::get_or_create(&pool, &key, "Key").await.unwrap();

    let now = 20_000_i64 * DAY_SECONDS;
    for (days_ago, close) in [(0_i64, 60.0), (1, 55.0), (7, 50.0)] {
        let day = now / DAY_SECONDS - days_ago;
        let ts = day * DAY_SECONDS;
        PricePointsRepo::insert(
            &pool,
            &InsertPricePoint {
                item_id,
                ts,
                source: "snapshot",
                best_buy_keys: None,
                best_buy_ref: Some(close),
                best_sell_keys: None,
                best_sell_ref: Some(close),
                buy_count: Some(1),
                sell_count: Some(1),
                key_rate_ref: 60.0,
            },
        )
        .await
        .unwrap();
        PriceDailyRepo::recompute_day(&pool, item_id, day)
            .await
            .unwrap();
    }

    let valuation = value_item_key(&pool, &key, "fallback", now).await.unwrap();

    assert_eq!(valuation.history_days, 3);
    assert!(valuation.trend_d1_pct.is_some());
    assert!(valuation.trend_d7_pct.is_some());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
