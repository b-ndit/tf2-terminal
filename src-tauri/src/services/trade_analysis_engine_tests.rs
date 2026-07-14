use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::infra::db;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::{MarketListingsRepo, UpsertMarketListing};

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-trade-analysis-engine-test-{}-{}",
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

fn tf2_item_from_json(json: &str) -> TF2Item {
    serde_json::from_str(json).unwrap()
}

// `value_item_key` itself is now tested in `item_valuation_tests.rs`
// (Module 11 extracted it into `services::item_valuation`, shared with
// `flip_finder`) — the tests below only exercise this module's own
// given/received-side resolution logic, which calls it.

#[tokio::test]
async fn value_given_side_resolves_cached_assets_and_flags_unresolved() {
    let (pool, dir) = test_pool().await;
    let key = plain_key(5021);
    let item_id = ItemsRepo::get_or_create(&pool, &key, "Mann Co. Supply Crate Key")
        .await
        .unwrap();
    seed_listing(&pool, "l1", &key, "sell", 60.0, 1000).await;

    let mut my_item_ids = HashMap::new();
    my_item_ids.insert("asset-known".to_string(), item_id);

    let assets = vec![
        TradeOfferAsset {
            assetid: "asset-known".to_string(),
        },
        TradeOfferAsset {
            assetid: "asset-missing".to_string(),
        },
    ];

    let result = value_given_side(&pool, &my_item_ids, &assets, 2000)
        .await
        .unwrap();

    assert_eq!(result.views.len(), 2);
    assert_eq!(result.views[0].name, "Mann Co. Supply Crate Key");
    assert!(result.views[0].estimated_ref.is_some());
    assert_eq!(result.views[1].name, "Unresolved Item");
    assert_eq!(result.views[1].estimated_ref, None);
    assert_eq!(result.trade_side.items.len(), 2);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn value_received_side_resolves_partner_items_and_flags_unresolved() {
    let (pool, dir) = test_pool().await;

    let item = tf2_item_from_json(r#"{"id": 555, "defindex": 45, "quality": 6}"#);
    let mut partner_items = HashMap::new();
    partner_items.insert("555".to_string(), item);

    let assets = vec![
        TradeOfferAsset {
            assetid: "555".to_string(),
        },
        TradeOfferAsset {
            assetid: "unknown".to_string(),
        },
    ];

    let result = value_received_side(&pool, Some(&partner_items), &assets, 1000)
        .await
        .unwrap();

    assert_eq!(result.views.len(), 2);
    // No schema/inventory sync has named defindex 45 in this test DB, so
    // the fallback name kicks in.
    assert_eq!(result.views[0].name, "Unknown Item 45");
    assert_eq!(result.views[1].name, "Unresolved Item");
    assert_eq!(result.views[1].estimated_ref, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn value_received_side_flags_everything_unresolved_when_partner_inventory_unavailable() {
    let (pool, dir) = test_pool().await;
    let assets = vec![TradeOfferAsset {
        assetid: "1".to_string(),
    }];

    let result = value_received_side(&pool, None, &assets, 1000)
        .await
        .unwrap();

    assert_eq!(result.views.len(), 1);
    assert_eq!(result.views[0].name, "Unresolved Item");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn index_by_asset_id_keys_items_by_their_string_asset_id() {
    let items = vec![tf2_item_from_json(
        r#"{"id": 42, "defindex": 1, "quality": 6}"#,
    )];
    let map = index_by_asset_id(items);
    assert!(map.contains_key("42"));
    assert_eq!(map.len(), 1);
}

#[tokio::test]
async fn fetch_partner_items_cached_returns_cached_items_without_a_network_call() {
    let (pool, dir) = test_pool().await;
    let partner = SteamId64::from_account_id(123);
    let cache_key = format!("{PARTNER_ITEMS_CACHE_KEY_PREFIX}{partner}");
    let items = vec![tf2_item_from_json(
        r#"{"id": 7, "defindex": 2, "quality": 6}"#,
    )];
    let json = serde_json::to_vec(&items).unwrap();
    KvCacheRepo::set(&pool, &cache_key, &json, Duration::from_secs(60))
        .await
        .unwrap();

    let api = SteamApiClient::new();
    // "unused-key" is never sent anywhere — the cache hit short-circuits
    // before any HTTP call would be made.
    let result = fetch_partner_items_cached(&pool, &api, "unused-key", partner).await;

    let map = result.expect("cached entry should be returned");
    assert!(map.contains_key("7"));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
