use std::sync::Arc;

use super::*;
use crate::infra::db;

/// The real, compiled `sample-plugin` example (Module 14's SDK sample),
/// built once for this repo and committed as a test fixture — proves the
/// whole pipeline (install → capability-gated host calls → dispatch)
/// against genuine wasm, not a hand-written mock.
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample_plugin");

fn uniq_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

async fn test_env() -> (SqlitePool, AppPaths, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "tf2-terminal-plugin-service-test-{}-{}",
        std::process::id(),
        uniq_suffix()
    ));
    let paths = AppPaths {
        config_dir: base.join("config"),
        data_dir: base.join("data"),
    };
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let db_path = base.join("test.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    (pool, paths, base)
}

#[tokio::test]
async fn install_plugin_copies_files_and_persists_metadata() {
    let (pool, paths, base) = test_env().await;
    let runtime = PluginRuntime::new().unwrap();

    let summary = install_plugin(&paths, &pool, &runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();
    assert_eq!(summary.name, "sample-plugin");
    assert_eq!(summary.version, "0.1.0");
    assert!(summary.capabilities.contains(&"market:read".to_string()));
    assert!(summary.capabilities.contains(&"notify:send".to_string()));
    assert!(summary.events.contains(&"alert_fired".to_string()));
    assert!(summary.events.contains(&"market_provider".to_string()));
    assert!(summary.has_panel);
    assert!(summary.enabled);

    let install_dir = paths.plugins_dir().join("sample-plugin");
    assert!(install_dir.join("sample_plugin.wasm").is_file());
    assert!(install_dir.join("panel").join("index.html").is_file());
    assert!(runtime.is_loaded("sample-plugin"));
    assert!(PluginsRepo::find_by_name(&pool, "sample-plugin")
        .await
        .unwrap()
        .is_some());

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn install_plugin_rejects_a_missing_entry_file() {
    let (pool, paths, base) = test_env().await;
    let runtime = PluginRuntime::new().unwrap();

    let bad_source = base.join("bad-plugin-source");
    std::fs::create_dir_all(&bad_source).unwrap();
    std::fs::write(
        bad_source.join("plugin.toml"),
        "name = \"bad\"\nversion = \"1.0.0\"\nentry = \"missing.wasm\"\n",
    )
    .unwrap();

    let result = install_plugin(&paths, &pool, &runtime, bad_source.to_str().unwrap(), 1_000).await;
    assert!(result.is_err());

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn list_plugins_returns_installed_plugins() {
    let (pool, paths, base) = test_env().await;
    let runtime = PluginRuntime::new().unwrap();
    install_plugin(&paths, &pool, &runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();

    let plugins = list_plugins(&pool).await.unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "sample-plugin");

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn set_plugin_enabled_toggles_the_runtime_cache_and_db_row() {
    let (pool, paths, base) = test_env().await;
    let runtime = PluginRuntime::new().unwrap();
    install_plugin(&paths, &pool, &runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();
    assert!(runtime.is_loaded("sample-plugin"));

    set_plugin_enabled(&pool, &paths, &runtime, "sample-plugin", false)
        .await
        .unwrap();
    assert!(!runtime.is_loaded("sample-plugin"));
    assert!(
        !PluginsRepo::find_by_name(&pool, "sample-plugin")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    set_plugin_enabled(&pool, &paths, &runtime, "sample-plugin", true)
        .await
        .unwrap();
    assert!(runtime.is_loaded("sample-plugin"));
    assert!(
        PluginsRepo::find_by_name(&pool, "sample-plugin")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn uninstall_plugin_removes_the_row_and_directory() {
    let (pool, paths, base) = test_env().await;
    let runtime = PluginRuntime::new().unwrap();
    install_plugin(&paths, &pool, &runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();

    uninstall_plugin(&pool, &paths, &runtime, "sample-plugin")
        .await
        .unwrap();

    assert!(PluginsRepo::find_by_name(&pool, "sample-plugin")
        .await
        .unwrap()
        .is_none());
    assert!(!runtime.is_loaded("sample-plugin"));
    assert!(!paths.plugins_dir().join("sample-plugin").exists());

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn get_plugin_panel_path_returns_the_panel_file() {
    let (pool, paths, base) = test_env().await;
    let runtime = PluginRuntime::new().unwrap();
    install_plugin(&paths, &pool, &runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();

    let panel_path = get_plugin_panel_path(&pool, &paths, "sample-plugin")
        .await
        .unwrap()
        .expect("sample-plugin ships a panel");
    assert!(panel_path.ends_with("index.html"));
    assert!(std::path::Path::new(&panel_path).is_file());

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn get_plugin_panel_path_is_none_for_an_unknown_plugin() {
    let (pool, paths, base) = test_env().await;
    let panel_path = get_plugin_panel_path(&pool, &paths, "nonexistent")
        .await
        .unwrap();
    assert!(panel_path.is_none());
    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn dispatch_alert_to_plugins_runs_the_real_plugin_without_panicking() {
    let (pool, paths, base) = test_env().await;
    let runtime = Arc::new(PluginRuntime::new().unwrap());
    install_plugin(&paths, &pool, &runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();

    let event = AlertFired {
        rule_id: 1,
        item_name: "Mann Co. Supply Crate Key".to_string(),
        kind: "price_drop".to_string(),
        message: "price dropped 10%".to_string(),
        fired_ts: 1_000.0,
        channels: vec!["discord".to_string()],
    };

    // Exercises the full real-wasm pipeline: capability-gated
    // market_price_daily + notify_send calls inside the plugin. No
    // stronger assertion is possible here without introspecting host-side
    // logs — the DB/bus-observable path is covered by
    // `poll_market_providers_injects_listings_from_a_subscribed_plugin`.
    dispatch_alert_to_plugins(&pool, &runtime, &reqwest::Client::new(), &event).await;

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn poll_market_providers_injects_listings_from_a_subscribed_plugin() {
    let (pool, paths, base) = test_env().await;
    let runtime = Arc::new(PluginRuntime::new().unwrap());
    install_plugin(&paths, &pool, &runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();

    let market_data = Arc::new(MarketDataService::new());
    poll_market_providers(&pool, &runtime, &reqwest::Client::new(), &market_data).await;

    let events = market_data.recent_events().await;
    assert!(events
        .iter()
        .any(|e| e.listing_id == "sample-plugin:demo-1"));

    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn load_enabled_plugins_restores_previously_installed_plugins_at_startup() {
    let (pool, paths, base) = test_env().await;
    let install_runtime = PluginRuntime::new().unwrap();
    install_plugin(&paths, &pool, &install_runtime, FIXTURE_DIR, 1_000)
        .await
        .unwrap();

    // Simulate a restart: a fresh runtime with nothing loaded yet.
    let fresh_runtime = PluginRuntime::new().unwrap();
    assert!(!fresh_runtime.is_loaded("sample-plugin"));
    load_enabled_plugins(&paths, &pool, &fresh_runtime)
        .await
        .unwrap();
    assert!(fresh_runtime.is_loaded("sample-plugin"));

    std::fs::remove_dir_all(&base).ok();
}
