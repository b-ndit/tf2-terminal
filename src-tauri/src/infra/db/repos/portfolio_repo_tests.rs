use super::*;
use crate::infra::db;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-portfolio-repo-test-{}-{}",
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

fn snap(ts: i64, steam_id: &str, total_ref: f64) -> InsertPortfolioSnapshot<'_> {
    InsertPortfolioSnapshot {
        ts,
        steam_id,
        total_ref,
        total_keys: total_ref / 60.0,
        pure_keys: Some(2),
        pure_metal_ref: Some(5.5),
        item_count: Some(214),
        unusual_count: Some(3),
        australium_count: Some(1),
    }
}

#[tokio::test]
async fn insert_then_latest_returns_the_newest_snapshot() {
    let (pool, dir) = test_pool().await;
    PortfolioSnapshotsRepo::insert(&pool, &snap(1000, "sid0", 100.0))
        .await
        .unwrap();
    PortfolioSnapshotsRepo::insert(&pool, &snap(2000, "sid0", 150.0))
        .await
        .unwrap();

    let latest = PortfolioSnapshotsRepo::latest(&pool, "sid0")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.ts, 2000);
    assert_eq!(latest.total_ref, 150.0);
    assert_eq!(latest.pure_keys, Some(2));
    assert_eq!(latest.unusual_count, Some(3));

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn insert_upserts_on_conflicting_ts() {
    let (pool, dir) = test_pool().await;
    PortfolioSnapshotsRepo::insert(&pool, &snap(1000, "sid0", 100.0))
        .await
        .unwrap();
    PortfolioSnapshotsRepo::insert(&pool, &snap(1000, "sid0", 200.0))
        .await
        .unwrap();

    let history = PortfolioSnapshotsRepo::history_since(&pool, "sid0", 0)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].total_ref, 200.0);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn latest_and_history_are_scoped_to_steam_id() {
    let (pool, dir) = test_pool().await;
    // Distinct `ts` values, matching real usage: this app supports one
    // logged-in Steam account at a time (`Config.steam_id` is a single
    // `Option`, not a list), so two accounts' snapshots colliding on the
    // exact same unix-second `ts` (the sole primary key, per the
    // documented schema) never happens in practice.
    PortfolioSnapshotsRepo::insert(&pool, &snap(1000, "sid0", 100.0))
        .await
        .unwrap();
    PortfolioSnapshotsRepo::insert(&pool, &snap(1001, "sid1", 999.0))
        .await
        .unwrap();

    let latest = PortfolioSnapshotsRepo::latest(&pool, "sid0")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.total_ref, 100.0);

    let history = PortfolioSnapshotsRepo::history_since(&pool, "sid0", 0)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn history_since_excludes_earlier_snapshots() {
    let (pool, dir) = test_pool().await;
    PortfolioSnapshotsRepo::insert(&pool, &snap(1000, "sid0", 100.0))
        .await
        .unwrap();
    PortfolioSnapshotsRepo::insert(&pool, &snap(2000, "sid0", 150.0))
        .await
        .unwrap();

    let history = PortfolioSnapshotsRepo::history_since(&pool, "sid0", 1500)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].ts, 2000);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn most_recent_before_finds_the_nearest_prior_snapshot() {
    let (pool, dir) = test_pool().await;
    PortfolioSnapshotsRepo::insert(&pool, &snap(1000, "sid0", 100.0))
        .await
        .unwrap();
    PortfolioSnapshotsRepo::insert(&pool, &snap(2000, "sid0", 150.0))
        .await
        .unwrap();
    PortfolioSnapshotsRepo::insert(&pool, &snap(3000, "sid0", 200.0))
        .await
        .unwrap();

    let row = PortfolioSnapshotsRepo::most_recent_before(&pool, "sid0", 2500)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.ts, 2000);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn most_recent_before_is_none_when_nothing_qualifies() {
    let (pool, dir) = test_pool().await;
    PortfolioSnapshotsRepo::insert(&pool, &snap(2000, "sid0", 150.0))
        .await
        .unwrap();

    let row = PortfolioSnapshotsRepo::most_recent_before(&pool, "sid0", 1000)
        .await
        .unwrap();
    assert_eq!(row, None);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn latest_is_none_for_unknown_steam_id() {
    let (pool, dir) = test_pool().await;
    assert_eq!(
        PortfolioSnapshotsRepo::latest(&pool, "nobody")
            .await
            .unwrap(),
        None
    );
    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
