use super::*;
use crate::infra::db;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-trades-repo-test-{}-{}",
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

fn trade<'a>(trade_offer_id: &'a str, completed_ts: i64, net_value_ref: f64) -> InsertTrade<'a> {
    InsertTrade {
        trade_offer_id,
        partner_steam_id: "76561198000000001",
        completed_ts,
        given_json: r#"[{"name":"Key","value_ref":60.0}]"#,
        received_json: r#"[{"name":"Hat","value_ref":80.0}]"#,
        net_value_ref,
    }
}

#[tokio::test]
async fn insert_if_new_inserts_and_reports_it_was_new() {
    let (pool, dir) = test_pool().await;
    let inserted = TradesRepo::insert_if_new(&pool, &trade("111", 1000, 20.0))
        .await
        .unwrap();
    assert!(inserted);

    let trades = TradesRepo::list_recent(&pool, 10).await.unwrap();
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].trade_offer_id, "111");
    assert_eq!(trades[0].net_value_ref, 20.0);
    assert_eq!(trades[0].rating, None);
    assert_eq!(trades[0].notes, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn insert_if_new_is_idempotent_for_the_same_trade_offer_id() {
    let (pool, dir) = test_pool().await;
    assert!(TradesRepo::insert_if_new(&pool, &trade("111", 1000, 20.0))
        .await
        .unwrap());
    assert!(
        !TradesRepo::insert_if_new(&pool, &trade("111", 1000, 999.0))
            .await
            .unwrap()
    );

    let trades = TradesRepo::list_recent(&pool, 10).await.unwrap();
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].net_value_ref, 20.0); // first write wins, second was ignored

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_recent_orders_newest_first_and_respects_limit() {
    let (pool, dir) = test_pool().await;
    for (id, ts) in [("a", 1000), ("b", 3000), ("c", 2000)] {
        TradesRepo::insert_if_new(&pool, &trade(id, ts, 1.0))
            .await
            .unwrap();
    }

    let trades = TradesRepo::list_recent(&pool, 2).await.unwrap();
    assert_eq!(trades.len(), 2);
    assert_eq!(trades[0].trade_offer_id, "b");
    assert_eq!(trades[1].trade_offer_id, "c");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn set_rating_and_set_notes_update_the_row() {
    let (pool, dir) = test_pool().await;
    TradesRepo::insert_if_new(&pool, &trade("111", 1000, 20.0))
        .await
        .unwrap();

    TradesRepo::set_rating(&pool, "111", Some(5)).await.unwrap();
    TradesRepo::set_notes(&pool, "111", Some("great trade"))
        .await
        .unwrap();

    let trades = TradesRepo::list_recent(&pool, 10).await.unwrap();
    assert_eq!(trades[0].rating, Some(5));
    assert_eq!(trades[0].notes, Some("great trade".to_string()));

    TradesRepo::set_rating(&pool, "111", None).await.unwrap();
    let trades = TradesRepo::list_recent(&pool, 10).await.unwrap();
    assert_eq!(trades[0].rating, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_recent_is_empty_when_no_trades_exist() {
    let (pool, dir) = test_pool().await;
    assert!(TradesRepo::list_recent(&pool, 10).await.unwrap().is_empty());
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
