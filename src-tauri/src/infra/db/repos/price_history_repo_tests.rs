use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::items_repo::ItemsRepo;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-price-history-repo-test-{}-{}",
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

fn point(item_id: i64, ts: i64, buy: Option<f64>, sell: Option<f64>) -> InsertPricePoint<'static> {
    InsertPricePoint {
        item_id,
        ts,
        source: "snapshot",
        best_buy_keys: None,
        best_buy_ref: buy,
        best_sell_keys: None,
        best_sell_ref: sell,
        buy_count: Some(1),
        sell_count: Some(1),
        key_rate_ref: 60.0,
    }
}

#[tokio::test]
async fn insert_and_history_since_round_trip() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    PricePointsRepo::insert(&pool, &point(item_id, 100, Some(58.0), Some(62.0)))
        .await
        .unwrap();
    PricePointsRepo::insert(&pool, &point(item_id, 200, Some(59.0), Some(63.0)))
        .await
        .unwrap();

    let history = PricePointsRepo::history_since(&pool, item_id, 0)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].ts, 100);
    assert_eq!(history[1].ts, 200);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn history_since_excludes_points_before_cutoff() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    PricePointsRepo::insert(&pool, &point(item_id, 100, Some(58.0), Some(62.0)))
        .await
        .unwrap();
    PricePointsRepo::insert(&pool, &point(item_id, 200, Some(59.0), Some(63.0)))
        .await
        .unwrap();

    let history = PricePointsRepo::history_since(&pool, item_id, 150)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].ts, 200);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn recompute_day_derives_ohlc_from_midpoint_values() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;
    const DAY: i64 = 19_000; // arbitrary day index
    let day_start = DAY * 86_400;

    // Midpoints: 60, 65, 55 -> open=60, high=65, low=55, close=55
    PricePointsRepo::insert(&pool, &point(item_id, day_start, Some(58.0), Some(62.0)))
        .await
        .unwrap();
    PricePointsRepo::insert(
        &pool,
        &point(item_id, day_start + 3600, Some(63.0), Some(67.0)),
    )
    .await
    .unwrap();
    PricePointsRepo::insert(
        &pool,
        &point(item_id, day_start + 7200, Some(53.0), Some(57.0)),
    )
    .await
    .unwrap();

    PriceDailyRepo::recompute_day(&pool, item_id, DAY)
        .await
        .unwrap();

    let daily = PriceDailyRepo::history_since(&pool, item_id, 0)
        .await
        .unwrap();
    assert_eq!(daily.len(), 1);
    let bar = &daily[0];
    assert_eq!(bar.day, DAY);
    assert_eq!(bar.open_ref, Some(60.0));
    assert_eq!(bar.high_ref, Some(65.0));
    assert_eq!(bar.low_ref, Some(55.0));
    assert_eq!(bar.close_ref, Some(55.0));
    assert_eq!(bar.median_ref, Some(60.0));
    assert_eq!(bar.samples, Some(3));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn recompute_day_uses_whichever_side_is_present() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;
    const DAY: i64 = 19_001;
    let day_start = DAY * 86_400;

    PricePointsRepo::insert(&pool, &point(item_id, day_start, Some(50.0), None))
        .await
        .unwrap();
    PricePointsRepo::insert(&pool, &point(item_id, day_start + 60, None, Some(70.0)))
        .await
        .unwrap();

    PriceDailyRepo::recompute_day(&pool, item_id, DAY)
        .await
        .unwrap();

    let daily = PriceDailyRepo::history_since(&pool, item_id, 0)
        .await
        .unwrap();
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].open_ref, Some(50.0));
    assert_eq!(daily[0].close_ref, Some(70.0));
    assert_eq!(daily[0].samples, Some(2));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn recompute_day_is_a_noop_when_no_points_exist() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    PriceDailyRepo::recompute_day(&pool, item_id, 12_345)
        .await
        .unwrap();

    let daily = PriceDailyRepo::history_since(&pool, item_id, 0)
        .await
        .unwrap();
    assert!(daily.is_empty());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn recompute_day_upserts_rather_than_duplicates() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;
    const DAY: i64 = 19_002;
    let day_start = DAY * 86_400;

    PricePointsRepo::insert(&pool, &point(item_id, day_start, Some(50.0), Some(50.0)))
        .await
        .unwrap();
    PriceDailyRepo::recompute_day(&pool, item_id, DAY)
        .await
        .unwrap();

    PricePointsRepo::insert(
        &pool,
        &point(item_id, day_start + 60, Some(90.0), Some(90.0)),
    )
    .await
    .unwrap();
    PriceDailyRepo::recompute_day(&pool, item_id, DAY)
        .await
        .unwrap();

    let daily = PriceDailyRepo::history_since(&pool, item_id, 0)
        .await
        .unwrap();
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].close_ref, Some(90.0));
    assert_eq!(daily[0].samples, Some(2));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn daily_history_since_excludes_days_before_cutoff() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    for day in [19_010_i64, 19_011] {
        let ts = day * 86_400;
        PricePointsRepo::insert(&pool, &point(item_id, ts, Some(50.0), Some(50.0)))
            .await
            .unwrap();
        PriceDailyRepo::recompute_day(&pool, item_id, day)
            .await
            .unwrap();
    }

    let daily = PriceDailyRepo::history_since(&pool, item_id, 19_011)
        .await
        .unwrap();
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].day, 19_011);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn latest_key_rate_is_none_before_any_observation() {
    let (pool, dir) = test_pool().await;
    assert_eq!(PricePointsRepo::latest_key_rate(&pool).await.unwrap(), None);
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn latest_key_rate_returns_the_most_recently_captured_rate() {
    let (pool, dir) = test_pool().await;
    let item_id = seed_item(&pool).await;

    PricePointsRepo::insert(
        &pool,
        &InsertPricePoint {
            key_rate_ref: 55.0,
            ..point(item_id, 1_000, Some(50.0), Some(50.0))
        },
    )
    .await
    .unwrap();
    PricePointsRepo::insert(
        &pool,
        &InsertPricePoint {
            key_rate_ref: 63.5,
            ..point(item_id, 2_000, Some(50.0), Some(50.0))
        },
    )
    .await
    .unwrap();

    let rate = PricePointsRepo::latest_key_rate(&pool).await.unwrap();
    assert_eq!(rate, Some(63.5));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
