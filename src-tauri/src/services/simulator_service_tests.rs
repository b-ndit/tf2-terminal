use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::inventory_repo::UpsertInventoryItem;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::{MarketListingsRepo, UpsertMarketListing};

const STEAM_ID: &str = "76561198000000000";
const NOW_TS: i64 = 20_000 * 86_400;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-simulator-service-test-{}-{}",
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

fn key_input(defindex: u32) -> ItemKeyInput {
    ItemKeyInput {
        defindex,
        quality: Quality::Unique as u8,
        effect_id: None,
        killstreak_tier: 0,
        australium: false,
        festivized: false,
        craftable: true,
    }
}

async fn seed_owned(pool: &SqlitePool, asset_id: &str, item_id: i64) {
    InventoryRepo::upsert(
        pool,
        &UpsertInventoryItem {
            asset_id,
            item_id,
            steam_id: STEAM_ID,
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
    key: &ItemKey,
    intent: &str,
    price: f64,
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
            updated_at: NOW_TS,
        },
    )
    .await
    .unwrap();
}

#[test]
fn item_key_input_converts_to_a_valid_item_key() {
    let key = ItemKey::try_from(key_input(100)).unwrap();
    assert_eq!(key.defindex, 100);
    assert_eq!(key.quality, Quality::Unique);
}

#[test]
fn item_key_input_rejects_an_invalid_quality() {
    let mut input = key_input(100);
    input.quality = 99;
    assert!(ItemKey::try_from(input).is_err());
}

#[tokio::test]
async fn simulate_trade_values_both_sides_of_a_hypothetical_trade() {
    let (pool, dir) = test_pool().await;

    let given_key = plain_key(200);
    let given_item_id = ItemsRepo::get_or_create(&pool, &given_key, "Given Hat")
        .await
        .unwrap();
    seed_listing(&pool, "given-sell", &given_key, "sell", 50.0).await;
    seed_owned(&pool, "asset-1", given_item_id).await;

    let received_key = plain_key(300);
    ItemsRepo::get_or_create(&pool, &received_key, "Received Hat")
        .await
        .unwrap();
    seed_listing(&pool, "received-sell", &received_key, "sell", 80.0).await;

    let result = simulate_trade(
        &pool,
        STEAM_ID,
        &["asset-1".to_string()],
        &[key_input(300)],
        NOW_TS,
    )
    .await
    .unwrap();

    assert_eq!(result.given_items.len(), 1);
    assert_eq!(result.given_items[0].name, "Given Hat");
    assert_eq!(result.received_items.len(), 1);
    assert_eq!(result.received_items[0].name, "Received Hat");
    assert!((result.given_total_ref - 50.0).abs() < 1e-9);
    assert!((result.received_total_ref - 80.0).abs() < 1e-9);
    assert!((result.net_ref - 30.0).abs() < 1e-9);
    assert!(result.stars >= 1 && result.stars <= 5);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn simulate_trade_marks_an_unknown_asset_id_as_unresolved() {
    let (pool, dir) = test_pool().await;

    let result = simulate_trade(&pool, STEAM_ID, &["never-synced".to_string()], &[], NOW_TS)
        .await
        .unwrap();

    assert_eq!(result.given_items.len(), 1);
    assert_eq!(result.given_items[0].name, "Unresolved Item");
    assert_eq!(result.given_items[0].estimated_ref, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn simulate_trade_marks_an_invalid_item_key_as_unresolved() {
    let (pool, dir) = test_pool().await;

    let mut invalid = key_input(100);
    invalid.quality = 99;

    let result = simulate_trade(&pool, STEAM_ID, &[], &[invalid], NOW_TS)
        .await
        .unwrap();

    assert_eq!(result.received_items.len(), 1);
    assert_eq!(result.received_items[0].name, "Unresolved Item");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn simulate_trade_handles_both_sides_empty() {
    let (pool, dir) = test_pool().await;

    let result = simulate_trade(&pool, STEAM_ID, &[], &[], NOW_TS)
        .await
        .unwrap();

    assert_eq!(result.given_items.len(), 0);
    assert_eq!(result.received_items.len(), 0);
    assert_eq!(result.net_ref, 0.0);
    assert_eq!(result.roi_pct, None);
    assert_eq!(result.counteroffer_additional_ref, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
