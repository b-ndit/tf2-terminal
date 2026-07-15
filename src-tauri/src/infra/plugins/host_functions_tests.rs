use std::sync::Arc;

use super::*;
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::error::AppResult;
use crate::infra::db;
use crate::infra::db::repos::inventory_repo::{InventoryRepo, UpsertInventoryItem};
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::price_history_repo::{
    InsertPricePoint, PriceDailyRepo, PricePointsRepo,
};
use crate::infra::plugins::runtime::{HostContextParams, PluginRuntime};

fn uniq_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

async fn test_pool() -> (sqlx::SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-host-functions-test-{}-{}",
        std::process::id(),
        uniq_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    (pool, dir)
}

fn write_wat(wat: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-host-functions-wasm-{}-{}",
        std::process::id(),
        uniq_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let wasm_path = dir.join("plugin.wasm");
    std::fs::write(&wasm_path, wat::parse_str(wat).unwrap()).unwrap();
    (wasm_path, dir)
}

async fn run_status(
    wat: &str,
    granted: Vec<Capability>,
    pool: sqlx::SqlitePool,
    steam_id: Option<String>,
) -> AppResult<i32> {
    let (wasm_path, wasm_dir) = write_wat(wat);
    let runtime = Arc::new(PluginRuntime::new().unwrap());
    runtime.load("under-test", &wasm_path).unwrap();

    let params = HostContextParams {
        granted,
        db: pool,
        http: reqwest::Client::new(),
        tokio_handle: tokio::runtime::Handle::current(),
        steam_id,
    };

    let result = {
        let runtime = runtime.clone();
        tokio::task::spawn_blocking(move || {
            runtime.call_export_json("under-test", params, "run", None)
        })
        .await
        .unwrap()?
    };

    std::fs::remove_dir_all(&wasm_dir).ok();
    let bytes = result.expect("status buffer should always be written");
    Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
}

/// Each test guest here calls exactly one host import with fixed
/// arguments and relays the raw `i32` status/length code back as a 4-byte
/// little-endian buffer — sidestepping this module's signed packed-`i64`
/// convention (meant for real payloads, not raw status codes) so tests can
/// decode negative error codes unambiguously.
fn market_price_wat() -> String {
    r#"
    (module
      (import "host" "host_market_price_daily" (func $target (param i32 i32 i32) (result i32)))
      (memory (export "memory") 2)
      (global $bump (mut i32) (i32.const 65536))
      (func $alloc (export "alloc") (param $len i32) (result i32)
        (local $ptr i32)
        (local.set $ptr (global.get $bump))
        (global.set $bump (i32.add (global.get $bump) (local.get $len)))
        (local.get $ptr))
      (func (export "dealloc") (param i32 i32))
      (func (export "run") (param i32 i32) (result i64)
        (local $status i32)
        (local $ptr i32)
        (local.set $status (call $target (i32.const 111) (i32.const 6) (i32.const 60000)))
        (local.set $ptr (call $alloc (i32.const 4)))
        (i32.store (local.get $ptr) (local.get $status))
        (i64.or (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32)) (i64.extend_i32_u (i32.const 4)))))
    "#
    .to_string()
}

fn inventory_list_wat() -> String {
    r#"
    (module
      (import "host" "host_inventory_list" (func $target (param i32) (result i32)))
      (memory (export "memory") 2)
      (global $bump (mut i32) (i32.const 65536))
      (func $alloc (export "alloc") (param $len i32) (result i32)
        (local $ptr i32)
        (local.set $ptr (global.get $bump))
        (global.set $bump (i32.add (global.get $bump) (local.get $len)))
        (local.get $ptr))
      (func (export "dealloc") (param i32 i32))
      (func (export "run") (param i32 i32) (result i64)
        (local $status i32)
        (local $ptr i32)
        (local.set $status (call $target (i32.const 60000)))
        (local.set $ptr (call $alloc (i32.const 4)))
        (i32.store (local.get $ptr) (local.get $status))
        (i64.or (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32)) (i64.extend_i32_u (i32.const 4)))))
    "#
    .to_string()
}

fn http_fetch_wat(url: &str) -> String {
    format!(
        r#"
        (module
          (import "host" "host_http_fetch" (func $target (param i32 i32 i32) (result i32)))
          (memory (export "memory") 2)
          (global $bump (mut i32) (i32.const 65536))
          (data (i32.const 0) "{url}")
          (func $alloc (export "alloc") (param $len i32) (result i32)
            (local $ptr i32)
            (local.set $ptr (global.get $bump))
            (global.set $bump (i32.add (global.get $bump) (local.get $len)))
            (local.get $ptr))
          (func (export "dealloc") (param i32 i32))
          (func (export "run") (param i32 i32) (result i64)
            (local $status i32)
            (local $ptr i32)
            (local.set $status (call $target (i32.const 0) (i32.const {url_len}) (i32.const 60000)))
            (local.set $ptr (call $alloc (i32.const 4)))
            (i32.store (local.get $ptr) (local.get $status))
            (i64.or (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32)) (i64.extend_i32_u (i32.const 4)))))
        "#,
        url = url,
        url_len = url.len(),
    )
}

fn notify_send_wat() -> String {
    r#"
    (module
      (import "host" "host_notify_send" (func $target (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 2)
      (global $bump (mut i32) (i32.const 65536))
      (data (i32.const 0) "title")
      (data (i32.const 16) "body")
      (func $alloc (export "alloc") (param $len i32) (result i32)
        (local $ptr i32)
        (local.set $ptr (global.get $bump))
        (global.set $bump (i32.add (global.get $bump) (local.get $len)))
        (local.get $ptr))
      (func (export "dealloc") (param i32 i32))
      (func (export "run") (param i32 i32) (result i64)
        (local $status i32)
        (local $ptr i32)
        (local.set $status (call $target (i32.const 0) (i32.const 5) (i32.const 16) (i32.const 4)))
        (local.set $ptr (call $alloc (i32.const 4)))
        (i32.store (local.get $ptr) (local.get $status))
        (i64.or (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32)) (i64.extend_i32_u (i32.const 4)))))
    "#
    .to_string()
}

#[tokio::test]
async fn market_price_daily_is_denied_without_the_capability() {
    let (pool, dir) = test_pool().await;
    let status = run_status(&market_price_wat(), Vec::new(), pool, None)
        .await
        .unwrap();
    assert_eq!(status, -1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn market_price_daily_returns_zero_when_the_item_has_no_price_history() {
    let (pool, dir) = test_pool().await;
    let status = run_status(
        &market_price_wat(),
        vec![Capability::MarketRead],
        pool,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status, 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn market_price_daily_returns_a_positive_length_when_data_exists() {
    let (pool, dir) = test_pool().await;
    let key = ItemKey {
        defindex: 111,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    let item_id = ItemsRepo::get_or_create(&pool, &key, "Test Item")
        .await
        .unwrap();
    let point_id = PricePointsRepo::insert(
        &pool,
        &InsertPricePoint {
            item_id,
            ts: 1_000,
            source: "test",
            best_buy_ref: Some(10.0),
            best_buy_keys: None,
            best_sell_ref: Some(12.0),
            best_sell_keys: None,
            buy_count: Some(1),
            sell_count: Some(1),
            key_rate_ref: 60.0,
        },
    )
    .await
    .unwrap();
    let _ = point_id;
    PriceDailyRepo::recompute_day(&pool, item_id, 1_000 / 86_400)
        .await
        .unwrap();

    let status = run_status(
        &market_price_wat(),
        vec![Capability::MarketRead],
        pool,
        None,
    )
    .await
    .unwrap();
    assert!(status > 0, "expected a positive JSON length, got {status}");
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn inventory_list_is_denied_without_the_capability() {
    let (pool, dir) = test_pool().await;
    let status = run_status(
        &inventory_list_wat(),
        Vec::new(),
        pool,
        Some("76500".to_string()),
    )
    .await
    .unwrap();
    assert_eq!(status, -1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn inventory_list_is_internal_error_when_nobody_is_logged_in() {
    let (pool, dir) = test_pool().await;
    let status = run_status(
        &inventory_list_wat(),
        vec![Capability::InventoryRead],
        pool,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status, -3);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn inventory_list_returns_an_empty_json_array_for_an_empty_inventory() {
    let (pool, dir) = test_pool().await;
    let status = run_status(
        &inventory_list_wat(),
        vec![Capability::InventoryRead],
        pool,
        Some("76500".to_string()),
    )
    .await
    .unwrap();
    // "[]" is 2 bytes — a positive, non-zero length distinguishes a real
    // (if empty) result from the "no data" `0` sentinel.
    assert_eq!(status, 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn inventory_list_includes_synced_items() {
    let (pool, dir) = test_pool().await;
    let key = ItemKey {
        defindex: 200,
        quality: Quality::Unique,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };
    let item_id = ItemsRepo::get_or_create(&pool, &key, "Some Hat")
        .await
        .unwrap();
    InventoryRepo::upsert(
        &pool,
        &UpsertInventoryItem {
            asset_id: "asset-1",
            item_id,
            steam_id: "76500",
            craft_number: None,
            paint_id: None,
            strange_count: None,
            tradable: true,
            marketable: Some(true),
            last_seen_ts: 1_000,
            raw_json: "{}",
        },
    )
    .await
    .unwrap();

    let status = run_status(
        &inventory_list_wat(),
        vec![Capability::InventoryRead],
        pool,
        Some("76500".to_string()),
    )
    .await
    .unwrap();
    assert!(
        status > 2,
        "expected more than an empty array, got {status}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn http_fetch_is_denied_without_an_allowlisted_domain() {
    let (pool, dir) = test_pool().await;
    let status = run_status(
        &http_fetch_wat("http://example.com/"),
        vec![Capability::Http(vec!["other.example.com".to_string()])],
        pool,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status, -1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn http_fetch_is_denied_when_no_http_capability_at_all() {
    let (pool, dir) = test_pool().await;
    let status = run_status(
        &http_fetch_wat("http://example.com/"),
        Vec::new(),
        pool,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status, -1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn notify_send_is_denied_without_the_capability() {
    let (pool, dir) = test_pool().await;
    let status = run_status(&notify_send_wat(), Vec::new(), pool, None)
        .await
        .unwrap();
    assert_eq!(status, -1);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn notify_send_is_best_effort_when_no_webhook_is_configured() {
    let (pool, dir) = test_pool().await;
    let status = run_status(&notify_send_wat(), vec![Capability::NotifySend], pool, None)
        .await
        .unwrap();
    // Best-effort: dropped silently (with a logged warning) rather than
    // surfaced as an error to the plugin, matching `alert_service`'s own
    // "no webhook configured" handling.
    assert_eq!(status, 0);
    std::fs::remove_dir_all(&dir).ok();
}

/// Fires many calls at once (rather than sequentially awaiting each one)
/// so they all land within milliseconds of each other — sequential calls
/// take long enough per round-trip (real wasmtime instantiation) that the
/// quota fully refills between them and never actually exhausts, which is
/// a property of `governor`'s token bucket, not a bug in this code.
#[tokio::test]
async fn market_read_calls_beyond_the_per_minute_quota_are_rate_limited() {
    let (pool, dir) = test_pool().await;
    let (wasm_path, wasm_dir) = write_wat(&market_price_wat());
    let runtime = Arc::new(PluginRuntime::new().unwrap());
    runtime.load("under-test", &wasm_path).unwrap();

    let mut handles = Vec::new();
    for _ in 0..(RATE_LIMIT_PER_MINUTE_FOR_TESTS + 20) {
        let params = HostContextParams {
            granted: vec![Capability::MarketRead],
            db: pool.clone(),
            http: reqwest::Client::new(),
            tokio_handle: tokio::runtime::Handle::current(),
            steam_id: None,
        };
        let runtime = runtime.clone();
        handles.push(tokio::task::spawn_blocking(move || {
            runtime.call_export_json("under-test", params, "run", None)
        }));
    }

    let mut saw_rate_limited = false;
    for handle in handles {
        let result = handle.await.unwrap().unwrap().unwrap();
        let status = i32::from_le_bytes(result.try_into().unwrap());
        if status == -2 {
            saw_rate_limited = true;
        }
    }
    assert!(
        saw_rate_limited,
        "expected at least one call to hit the rate limit"
    );

    std::fs::remove_dir_all(&wasm_dir).ok();
    std::fs::remove_dir_all(&dir).ok();
}

// Mirrors `RATE_LIMIT_PER_MINUTE` in `runtime.rs` — kept as a separate
// constant here rather than `pub(crate)`-exposing the real one, since
// tests should catch a change in the real budget rather than silently
// tracking it.
const RATE_LIMIT_PER_MINUTE_FOR_TESTS: u32 = 120;
