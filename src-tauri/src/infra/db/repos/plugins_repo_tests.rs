use super::*;
use crate::infra::db;

async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-plugins-repo-test-{}-{}",
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

fn plugin<'a>(name: &'a str) -> InsertPlugin<'a> {
    InsertPlugin {
        name,
        version: "0.1.0",
        entry_file: "plugin.wasm",
        capabilities_json: r#"["market:read"]"#,
        events_json: r#"["alert_fired"]"#,
        has_panel: false,
        installed_ts: 1_000,
    }
}

#[tokio::test]
async fn insert_then_find_by_name_round_trips() {
    let (pool, dir) = test_pool().await;
    PluginsRepo::insert(&pool, &plugin("sample")).await.unwrap();

    let row = PluginsRepo::find_by_name(&pool, "sample")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.name, "sample");
    assert_eq!(row.version, "0.1.0");
    assert!(row.enabled);
    assert!(!row.has_panel);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn find_by_name_is_none_for_an_unknown_plugin() {
    let (pool, dir) = test_pool().await;
    assert!(PluginsRepo::find_by_name(&pool, "nope")
        .await
        .unwrap()
        .is_none());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_returns_every_installed_plugin_newest_first() {
    let (pool, dir) = test_pool().await;
    PluginsRepo::insert(
        &pool,
        &InsertPlugin {
            installed_ts: 1_000,
            ..plugin("first")
        },
    )
    .await
    .unwrap();
    PluginsRepo::insert(
        &pool,
        &InsertPlugin {
            installed_ts: 2_000,
            ..plugin("second")
        },
    )
    .await
    .unwrap();

    let rows = PluginsRepo::list(&pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "second");
    assert_eq!(rows[1].name, "first");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn list_enabled_excludes_disabled_plugins() {
    let (pool, dir) = test_pool().await;
    PluginsRepo::insert(&pool, &plugin("keep")).await.unwrap();
    PluginsRepo::insert(&pool, &plugin("drop")).await.unwrap();
    PluginsRepo::set_enabled(&pool, "drop", false)
        .await
        .unwrap();

    let rows = PluginsRepo::list_enabled(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "keep");

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn set_enabled_toggles_in_both_directions() {
    let (pool, dir) = test_pool().await;
    PluginsRepo::insert(&pool, &plugin("sample")).await.unwrap();

    PluginsRepo::set_enabled(&pool, "sample", false)
        .await
        .unwrap();
    assert!(
        !PluginsRepo::find_by_name(&pool, "sample")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    PluginsRepo::set_enabled(&pool, "sample", true)
        .await
        .unwrap();
    assert!(
        PluginsRepo::find_by_name(&pool, "sample")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn delete_removes_the_row() {
    let (pool, dir) = test_pool().await;
    PluginsRepo::insert(&pool, &plugin("sample")).await.unwrap();
    PluginsRepo::delete(&pool, "sample").await.unwrap();

    assert!(PluginsRepo::find_by_name(&pool, "sample")
        .await
        .unwrap()
        .is_none());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn insert_rejects_a_duplicate_name() {
    let (pool, dir) = test_pool().await;
    PluginsRepo::insert(&pool, &plugin("sample")).await.unwrap();
    let result = PluginsRepo::insert(&pool, &plugin("sample")).await;
    assert!(result.is_err());

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}
