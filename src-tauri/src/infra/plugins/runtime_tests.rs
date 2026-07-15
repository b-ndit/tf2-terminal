use super::*;
use crate::infra::db;

fn write_wat_fixture(wat: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-plugin-runtime-test-{}-{}",
        std::process::id(),
        uniq_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let wasm_path = dir.join("plugin.wasm");
    std::fs::write(&wasm_path, wat::parse_str(wat).unwrap()).unwrap();
    (wasm_path, dir)
}

fn uniq_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// A minimal but real ABI-conformant plugin: a bump allocator for
/// `alloc`/`dealloc`, and `plugin_init` returning a fixed 5-byte string
/// packed as `(ptr << 32) | len`, per this module's ABI convention.
const ECHO_HELLO_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))
  (func (export "alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $len)))
    (local.get $ptr))
  (func (export "dealloc") (param i32 i32))
  (data (i32.const 0) "hello")
  (func (export "plugin_init") (param i32 i32) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (i32.const 0)) (i64.const 32)) (i64.extend_i32_u (i32.const 5)))))
"#;

const INFINITE_LOOP_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "alloc") (param i32) (result i32) (i32.const 0))
  (func (export "dealloc") (param i32 i32))
  (func (export "spin") (param i32 i32) (result i64)
    (loop $l (br $l))
    (i64.const 0)))
"#;

async fn test_params() -> (HostContextParams, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tf2-terminal-plugin-runtime-db-{}-{}",
        std::process::id(),
        uniq_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let params = HostContextParams {
        granted: Vec::new(),
        db: pool,
        http: reqwest::Client::new(),
        tokio_handle: tokio::runtime::Handle::current(),
        steam_id: None,
    };
    (params, dir)
}

#[tokio::test]
async fn call_export_json_round_trips_a_guest_export() {
    let (wasm_path, wasm_dir) = write_wat_fixture(ECHO_HELLO_WAT);
    let (params, db_dir) = test_params().await;

    let runtime = PluginRuntime::new().unwrap();
    runtime.load("echo", &wasm_path).unwrap();

    let result = runtime
        .call_export_json("echo", params, "plugin_init", None)
        .unwrap();
    assert_eq!(result, Some(b"hello".to_vec()));

    std::fs::remove_dir_all(&wasm_dir).ok();
    std::fs::remove_dir_all(&db_dir).ok();
}

#[tokio::test]
async fn call_export_json_returns_none_for_an_export_the_plugin_does_not_implement() {
    let (wasm_path, wasm_dir) = write_wat_fixture(ECHO_HELLO_WAT);
    let (params, db_dir) = test_params().await;

    let runtime = PluginRuntime::new().unwrap();
    runtime.load("echo", &wasm_path).unwrap();

    let result = runtime
        .call_export_json("echo", params, "on_alert_fired", None)
        .unwrap();
    assert_eq!(result, None);

    std::fs::remove_dir_all(&wasm_dir).ok();
    std::fs::remove_dir_all(&db_dir).ok();
}

#[tokio::test]
async fn call_export_json_errors_for_a_plugin_that_was_never_loaded() {
    let (params, db_dir) = test_params().await;
    let runtime = PluginRuntime::new().unwrap();

    let result = runtime.call_export_json("nonexistent", params, "plugin_init", None);
    assert!(result.is_err());

    std::fs::remove_dir_all(&db_dir).ok();
}

#[tokio::test]
async fn unload_makes_further_calls_fail() {
    let (wasm_path, wasm_dir) = write_wat_fixture(ECHO_HELLO_WAT);
    let (params, db_dir) = test_params().await;

    let runtime = PluginRuntime::new().unwrap();
    runtime.load("echo", &wasm_path).unwrap();
    assert!(runtime.is_loaded("echo"));
    runtime.unload("echo");
    assert!(!runtime.is_loaded("echo"));

    let result = runtime.call_export_json("echo", params, "plugin_init", None);
    assert!(result.is_err());

    std::fs::remove_dir_all(&wasm_dir).ok();
    std::fs::remove_dir_all(&db_dir).ok();
}

/// Proves the sandbox is real: an infinite loop traps on fuel exhaustion
/// rather than hanging the host.
#[tokio::test]
async fn an_infinite_loop_export_traps_on_fuel_exhaustion() {
    let (wasm_path, wasm_dir) = write_wat_fixture(INFINITE_LOOP_WAT);
    let (params, db_dir) = test_params().await;

    let runtime = PluginRuntime::new().unwrap();
    runtime.load("spinner", &wasm_path).unwrap();

    let result = runtime.call_export_json("spinner", params, "spin", None);
    assert!(result.is_err());

    std::fs::remove_dir_all(&wasm_dir).ok();
    std::fs::remove_dir_all(&db_dir).ok();
}
