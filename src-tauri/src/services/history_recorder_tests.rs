use std::collections::HashMap;

use super::*;
use crate::infra::backpack_tf::models::{PriceCatalogItem, QualityPrices, TradableGroup};
use crate::infra::db;
use crate::infra::db::repos::market_listings_repo::UpsertMarketListing;
use crate::infra::db::repos::price_history_repo::PriceDailyRepo;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-history-recorder-test-{}-{}",
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
    defindex: i64,
    intent: &str,
    price_ref: f64,
) {
    MarketListingsRepo::upsert(
        pool,
        &UpsertMarketListing {
            listing_id,
            defindex,
            quality: Quality::Unique as u8 as i64,
            effect_id: None,
            killstreak_tier: 0,
            australium: false,
            festivized: false,
            craftable: true,
            intent,
            price_ref,
            steam_id: "sid0",
            steam_name: Some("Trader"),
            updated_at: 1000,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn snapshot_is_noop_without_a_known_key_rate() {
    let (pool, dir) = test_pool().await;
    ItemsRepo::get_or_create(&pool, &plain_key(5021), "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    ItemsRepo::get_or_create(&pool, &plain_key(5021), "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    // No Key listings at all yet -> no derivable rate.
    seed_listing(&pool, "l1", 200, "sell", 10.0).await;

    let recorded = HistoryRecorder::snapshot(&pool).await.unwrap();
    assert_eq!(recorded, 0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn snapshot_records_points_and_daily_rollup_for_known_items() {
    let (pool, dir) = test_pool().await;
    ItemsRepo::get_or_create(&pool, &plain_key(5021), "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    let scattergun_id = ItemsRepo::get_or_create(&pool, &plain_key(45), "Scattergun")
        .await
        .unwrap();

    // Key listings establish the going rate: mid(58, 62) = 60.
    seed_listing(&pool, "key_buy", 5021, "buy", 58.0).await;
    seed_listing(&pool, "key_sell", 5021, "sell", 62.0).await;
    seed_listing(&pool, "sg_buy", 45, "buy", 8.0).await;
    seed_listing(&pool, "sg_sell", 45, "sell", 10.0).await;

    let recorded = HistoryRecorder::snapshot(&pool).await.unwrap();
    assert_eq!(recorded, 2); // the key itself + the scattergun

    let history = PricePointsRepo::history_since(&pool, scattergun_id, 0)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].best_buy_ref, Some(8.0));
    assert_eq!(history[0].best_sell_ref, Some(10.0));

    let daily = PriceDailyRepo::history_since(&pool, scattergun_id, 0)
        .await
        .unwrap();
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].close_ref, Some(9.0)); // mid(8, 10)

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn snapshot_skips_items_with_no_matching_item_row() {
    let (pool, dir) = test_pool().await;
    ItemsRepo::get_or_create(&pool, &plain_key(5021), "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    seed_listing(&pool, "key_buy", 5021, "buy", 58.0).await;
    seed_listing(&pool, "key_sell", 5021, "sell", 62.0).await;
    // Unknown defindex — schema/inventory sync never seeded a name for it.
    seed_listing(&pool, "unknown_sell", 999_999, "sell", 5.0).await;

    let recorded = HistoryRecorder::snapshot(&pool).await.unwrap();
    assert_eq!(recorded, 1); // only the key itself

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

fn plain_entry(value: f64, currency: &str) -> PriceEntry {
    PriceEntry {
        value: Some(value),
        currency: Some(currency.to_string()),
        difference: None,
        last_update: None,
        value_high: None,
        value_raw: None,
    }
}

fn catalog_with(items: Vec<(&str, PriceCatalogItem)>) -> PriceCatalogResponse {
    PriceCatalogResponse {
        success: 1,
        current_time: 1000,
        items: items
            .into_iter()
            .map(|(name, item)| (name.to_string(), item))
            .collect(),
    }
}

fn key_catalog_item() -> PriceCatalogItem {
    let mut prices = HashMap::new();
    prices.insert(
        (Quality::Unique as u8).to_string(),
        QualityPrices {
            tradable: Some(TradableGroup {
                craftable: Some(CraftableEntry::Plain(vec![plain_entry(60.0, "metal")])),
                non_craftable: None,
            }),
            non_tradable: None,
        },
    );
    PriceCatalogItem {
        defindex: vec![5021],
        prices,
    }
}

#[tokio::test]
async fn record_schema_sync_is_noop_without_a_derivable_key_rate() {
    let (pool, dir) = test_pool().await;
    let mut prices = HashMap::new();
    prices.insert(
        (Quality::Unique as u8).to_string(),
        QualityPrices {
            tradable: Some(TradableGroup {
                craftable: Some(CraftableEntry::Plain(vec![plain_entry(10.0, "metal")])),
                non_craftable: None,
            }),
            non_tradable: None,
        },
    );
    let catalog = catalog_with(vec![(
        "Scattergun",
        PriceCatalogItem {
            defindex: vec![45],
            prices,
        },
    )]);

    let recorded = HistoryRecorder::record_schema_sync(&pool, &catalog)
        .await
        .unwrap();
    assert_eq!(recorded, 0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn record_schema_sync_records_known_items_and_skips_unknown() {
    let (pool, dir) = test_pool().await;
    let scattergun_id = ItemsRepo::get_or_create(&pool, &plain_key(45), "Scattergun")
        .await
        .unwrap();
    ItemsRepo::get_or_create(&pool, &plain_key(5021), "Mann Co. Supply Crate Key")
        .await
        .unwrap();

    let mut sg_prices = HashMap::new();
    sg_prices.insert(
        (Quality::Unique as u8).to_string(),
        QualityPrices {
            tradable: Some(TradableGroup {
                craftable: Some(CraftableEntry::Plain(vec![plain_entry(9.0, "metal")])),
                non_craftable: None,
            }),
            non_tradable: None,
        },
    );

    let catalog = catalog_with(vec![
        ("Mann Co. Supply Crate Key", key_catalog_item()),
        (
            "Scattergun",
            PriceCatalogItem {
                defindex: vec![45],
                prices: sg_prices,
            },
        ),
        (
            "Unseeded Widget",
            PriceCatalogItem {
                defindex: vec![777_777],
                prices: HashMap::new(),
            },
        ),
    ]);

    let recorded = HistoryRecorder::record_schema_sync(&pool, &catalog)
        .await
        .unwrap();
    // The key itself (its own catalog entry) + the scattergun.
    assert_eq!(recorded, 2);

    let history = PricePointsRepo::history_since(&pool, scattergun_id, 0)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].best_sell_ref, Some(9.0));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn record_schema_sync_converts_keys_denominated_prices_via_derived_rate() {
    let (pool, dir) = test_pool().await;
    let widget_id = ItemsRepo::get_or_create(&pool, &plain_key(9001), "Unusual Hat")
        .await
        .unwrap();

    let mut widget_prices = HashMap::new();
    widget_prices.insert(
        (Quality::Unique as u8).to_string(),
        QualityPrices {
            tradable: Some(TradableGroup {
                craftable: Some(CraftableEntry::Plain(vec![plain_entry(2.0, "keys")])),
                non_craftable: None,
            }),
            non_tradable: None,
        },
    );

    let catalog = catalog_with(vec![
        ("Mann Co. Supply Crate Key", key_catalog_item()),
        (
            "Widget",
            PriceCatalogItem {
                defindex: vec![9001],
                prices: widget_prices,
            },
        ),
    ]);

    HistoryRecorder::record_schema_sync(&pool, &catalog)
        .await
        .unwrap();

    let history = PricePointsRepo::history_since(&pool, widget_id, 0)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    // 2 keys at a derived rate of 60 ref/key = 120 ref.
    assert_eq!(history[0].best_sell_ref, Some(120.0));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
