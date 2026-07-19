use super::*;
use crate::infra::db;
use crate::infra::steam::trade_offers::TradeOfferAsset;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-trade-history-service-test-{}-{}",
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

fn offer(
    id: u64,
    accountid_other: u32,
    give_count: usize,
    receive_count: usize,
    time_updated: i64,
) -> TradeOffer {
    TradeOffer {
        tradeofferid: id,
        accountid_other,
        message: String::new(),
        trade_offer_state: 3,
        items_to_give: (0..give_count)
            .map(|i| TradeOfferAsset {
                assetid: format!("g{i}"),
            })
            .collect(),
        items_to_receive: (0..receive_count)
            .map(|i| TradeOfferAsset {
                assetid: format!("r{i}"),
            })
            .collect(),
        time_created: time_updated - 100,
        time_updated,
    }
}

async fn seed_cached_analysis(
    pool: &SqlitePool,
    trade_offer_id: &str,
    analysis: &CachedTradeAnalysis,
) {
    let json = serde_json::to_vec(analysis).unwrap();
    KvCacheRepo::set(
        pool,
        &trade_analysis_cache_key(trade_offer_id),
        &json,
        Duration::from_secs(3600),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn import_completed_offer_promotes_cached_analysis() {
    let (pool, dir) = test_pool().await;
    let analysis = CachedTradeAnalysis {
        partner_steam_id: "76561198000000099".to_string(),
        given: vec![TradeItemView {
            name: "Key".to_string(),
            estimated_ref: Some(60.0),
            asset_id: None,
            quality: None,
            effect_id: None,
            killstreak_tier: None,
            australium: None,
            festivized: None,
            paint_id: None,
            craft_number: None,
            strange_count: None,
            image_url: None,
        }],
        received: vec![TradeItemView {
            name: "Hat".to_string(),
            estimated_ref: Some(80.0),
            asset_id: None,
            quality: None,
            effect_id: None,
            killstreak_tier: None,
            australium: None,
            festivized: None,
            paint_id: None,
            craft_number: None,
            strange_count: None,
            image_url: None,
        }],
        net_ref: 20.0,
    };
    seed_cached_analysis(&pool, "111", &analysis).await;

    let inserted = import_completed_offer(&pool, &offer(111, 123, 1, 1, 5000))
        .await
        .unwrap();
    assert!(inserted);

    let trades = list_trades(&pool, 10).await.unwrap();
    assert_eq!(trades.len(), 1);
    let trade = &trades[0];
    assert_eq!(trade.trade_offer_id, "111");
    assert_eq!(trade.completed_ts, 5000.0);
    assert_eq!(trade.given.len(), 1);
    assert_eq!(trade.given[0].name, "Key");
    assert_eq!(trade.received[0].value_ref, Some(80.0));
    assert_eq!(trade.net_value_ref, 20.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn import_completed_offer_falls_back_to_unresolved_without_a_cached_analysis() {
    let (pool, dir) = test_pool().await;

    let inserted = import_completed_offer(&pool, &offer(222, 123, 2, 1, 5000))
        .await
        .unwrap();
    assert!(inserted);

    let trades = list_trades(&pool, 10).await.unwrap();
    assert_eq!(trades[0].given.len(), 2);
    assert!(trades[0]
        .given
        .iter()
        .all(|i| i.name == UNRESOLVED_ITEM_NAME && i.value_ref.is_none()));
    assert_eq!(trades[0].received.len(), 1);
    assert_eq!(trades[0].net_value_ref, 0.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn import_completed_offer_is_idempotent() {
    let (pool, dir) = test_pool().await;

    assert!(import_completed_offer(&pool, &offer(333, 123, 1, 1, 5000))
        .await
        .unwrap());
    assert!(!import_completed_offer(&pool, &offer(333, 123, 1, 1, 5000))
        .await
        .unwrap());

    let trades = list_trades(&pool, 10).await.unwrap();
    assert_eq!(trades.len(), 1);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn set_trade_rating_and_notes_update_the_ledger_entry() {
    let (pool, dir) = test_pool().await;
    import_completed_offer(&pool, &offer(444, 123, 1, 1, 5000))
        .await
        .unwrap();

    set_trade_rating(&pool, "444", Some(5)).await.unwrap();
    set_trade_notes(&pool, "444", Some("good deal"))
        .await
        .unwrap();

    let trades = list_trades(&pool, 10).await.unwrap();
    assert_eq!(trades[0].rating, Some(5));
    assert_eq!(trades[0].notes, Some("good deal".to_string()));

    set_trade_rating(&pool, "444", None).await.unwrap();
    let trades = list_trades(&pool, 10).await.unwrap();
    assert_eq!(trades[0].rating, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_trades_is_empty_when_nothing_imported() {
    let (pool, dir) = test_pool().await;
    assert!(list_trades(&pool, 10).await.unwrap().is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn last_sync_ts_defaults_to_the_backfill_window_when_never_synced() {
    let (pool, dir) = test_pool().await;
    let expected = now_unix() - DEFAULT_BACKFILL_DAYS * DAY_SECONDS;
    let ts = last_sync_ts(&pool).await.unwrap();
    assert!((ts - expected).abs() < 5);
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
