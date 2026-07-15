//! The wasmtime engine, compiled-module cache, and guest-memory ABI for
//! Module 14's plugin system.
//!
//! # ABI
//!
//! Every plugin targets `wasm32-wasip1` (so Rust `std` works) and must
//! export `memory`, `alloc(len: i32) -> i32`, and `dealloc(ptr: i32, len:
//! i32)`. Two data-crossing conventions, verified end-to-end against a
//! real compiled Rust plugin before this was written:
//!
//! - **Host calls a guest export** (`plugin_init`/`on_alert_fired`/
//!   `provide_listings`, via [`PluginRuntime::call_export_json`]): every
//!   such export has the fixed signature `(in_ptr: i32, in_len: i32) ->
//!   i64`. The host writes any input bytes into guest memory itself (by
//!   calling the guest's own `alloc`, the same trick used in the other
//!   direction below) before the call. The guest returns a packed `(ptr
//!   << 32) | len` (`0` means "nothing to return").
//! - **Guest calls a host import** (`host_*`, in [`super::host_functions`]):
//!   the guest passes an `out_ptr_ptr` — an i32 address of a 4-byte cell
//!   in its own memory. The host calls the guest's `alloc` to get a
//!   buffer, writes its JSON response there, writes that pointer into
//!   `out_ptr_ptr`, and returns the byte length (or a negative error
//!   code: `-1` capability denied, `-2` rate limited, `-3` internal).
//!
//! WASI is wired in only as a capability-less shim
//! (`WasiCtxBuilder::new()`'s defaults: closed stdin, discarded
//! stdout/stderr, no env, no args, no preopens) purely so `wasm32-wasip1`
//! binaries link and run — real host access only ever happens through the
//! capability-checked `host_*` imports. Fuel metering
//! (`Config::consume_fuel`) traps runaway/infinite-loop plugins instead of
//! hanging the host.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Arc, Mutex};

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use sqlx::SqlitePool;
use tokio::runtime::Handle;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::WasiCtxBuilder;

use crate::domain::plugin::Capability;
use crate::error::{AppError, AppResult};
use crate::infra::plugins::host_functions;

/// Generous but bounded — enough for any reasonable data-processing hook,
/// small enough that a genuinely infinite loop traps in well under a
/// second rather than hanging the calling `spawn_blocking` thread.
const FUEL_PER_CALL: u64 = 200_000_000;
/// Per-plugin budget for `host_*` calls within a single invocation — a
/// plugin can make several host calls per hook invocation (e.g. a price
/// lookup plus a notification) without tripping this in ordinary use.
const RATE_LIMIT_PER_MINUTE: u32 = 120;

/// Per-`Store` state — everything a `host_*` import closure needs to
/// decide whether to act and how to reach real I/O.
pub struct HostContext {
    pub plugin_name: String,
    pub granted: Vec<Capability>,
    pub db: SqlitePool,
    pub http: reqwest::Client,
    pub tokio_handle: Handle,
    pub steam_id: Option<String>,
    pub limiter: Arc<DefaultDirectRateLimiter>,
    pub(crate) wasi: WasiP1Ctx,
}

/// Everything [`PluginRuntime::call_export_json`] needs to build a fresh
/// [`HostContext`] for one call — the caller (`services::plugin_service`)
/// supplies these since they come from `AppState`, not the runtime itself.
pub struct HostContextParams {
    pub granted: Vec<Capability>,
    pub db: SqlitePool,
    pub http: reqwest::Client,
    pub tokio_handle: Handle,
    pub steam_id: Option<String>,
}

/// Owns the wasmtime `Engine`, the one shared `Linker` (host imports are
/// identical for every plugin — only the granted capabilities in each
/// call's [`HostContext`] differ), a compiled-`Module` cache, and a
/// per-plugin rate limiter for `host_*` calls.
pub struct PluginRuntime {
    engine: Engine,
    linker: Linker<HostContext>,
    modules: Mutex<HashMap<String, Module>>,
    limiters: Mutex<HashMap<String, Arc<DefaultDirectRateLimiter>>>,
}

impl PluginRuntime {
    pub fn new() -> AppResult<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|e| AppError::Internal(format!("failed to create wasm engine: {e}")))?;

        let mut linker = Linker::new(&engine);
        host_functions::register(&mut linker)
            .map_err(|e| AppError::Internal(format!("failed to build plugin host linker: {e}")))?;

        Ok(Self {
            engine,
            linker,
            modules: Mutex::new(HashMap::new()),
            limiters: Mutex::new(HashMap::new()),
        })
    }

    fn quota() -> Quota {
        Quota::per_minute(NonZeroU32::new(RATE_LIMIT_PER_MINUTE).expect("nonzero constant"))
    }

    /// Compiles `wasm_path` and caches it under `name`, replacing any
    /// previous entry (re-installing/updating a plugin re-loads it).
    pub fn load(&self, name: &str, wasm_path: &Path) -> AppResult<()> {
        let module = Module::from_file(&self.engine, wasm_path)
            .map_err(|e| AppError::Internal(format!("failed to compile plugin '{name}': {e}")))?;
        self.modules
            .lock()
            .expect("plugin module cache lock poisoned")
            .insert(name.to_string(), module);
        self.limiters
            .lock()
            .expect("plugin rate limiter cache lock poisoned")
            .insert(
                name.to_string(),
                Arc::new(RateLimiter::direct(Self::quota())),
            );
        Ok(())
    }

    pub fn unload(&self, name: &str) {
        self.modules
            .lock()
            .expect("plugin module cache lock poisoned")
            .remove(name);
        self.limiters
            .lock()
            .expect("plugin rate limiter cache lock poisoned")
            .remove(name);
    }

    #[cfg(test)]
    pub fn is_loaded(&self, name: &str) -> bool {
        self.modules
            .lock()
            .expect("plugin module cache lock poisoned")
            .contains_key(name)
    }

    /// Calls a guest export following this module's fixed ABI (see the
    /// module doc comment). `Ok(None)` both when the plugin doesn't export
    /// `export` at all (every hook is optional) and when it returns the
    /// empty result (packed `0`) — callers that need to distinguish "not
    /// implemented" from "implemented, nothing to say" don't need to in
    /// this system, since both mean "nothing happens".
    pub fn call_export_json(
        &self,
        plugin_name: &str,
        params: HostContextParams,
        export: &str,
        input: Option<&[u8]>,
    ) -> AppResult<Option<Vec<u8>>> {
        let module = {
            let modules = self
                .modules
                .lock()
                .expect("plugin module cache lock poisoned");
            modules.get(plugin_name).cloned()
        }
        .ok_or_else(|| AppError::Internal(format!("plugin '{plugin_name}' is not loaded")))?;

        let limiter = {
            let limiters = self
                .limiters
                .lock()
                .expect("plugin rate limiter cache lock poisoned");
            limiters
                .get(plugin_name)
                .cloned()
                .unwrap_or_else(|| Arc::new(RateLimiter::direct(Self::quota())))
        };

        let wasi = WasiCtxBuilder::new().build_p1();
        let ctx = HostContext {
            plugin_name: plugin_name.to_string(),
            granted: params.granted,
            db: params.db,
            http: params.http,
            tokio_handle: params.tokio_handle,
            steam_id: params.steam_id,
            limiter,
            wasi,
        };

        let mut store = Store::new(&self.engine, ctx);
        store
            .set_fuel(FUEL_PER_CALL)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let instance = self.linker.instantiate(&mut store, &module).map_err(|e| {
            AppError::Internal(format!("failed to instantiate plugin '{plugin_name}': {e}"))
        })?;

        let Ok(func) = instance.get_typed_func::<(i32, i32), i64>(&mut store, export) else {
            return Ok(None);
        };

        let (in_ptr, in_len) = match input {
            Some(bytes) if !bytes.is_empty() => write_into_guest(&instance, &mut store, bytes)?,
            _ => (0, 0),
        };

        let packed = func.call(&mut store, (in_ptr, in_len)).map_err(|e| {
            AppError::Internal(format!(
                "plugin '{plugin_name}' export '{export}' trapped: {e}"
            ))
        })?;

        if packed == 0 {
            return Ok(None);
        }
        let ptr = (packed >> 32) as i32;
        let len = (packed & 0xFFFF_FFFF) as i32;
        read_from_guest(&instance, &mut store, ptr, len).map(Some)
    }
}

/// Calls the guest's `alloc` export to obtain a buffer, writes `bytes`
/// into it, and returns `(ptr, len)` — the same convention used both when
/// the host hands input to a guest export and when a `host_*` import
/// hands its JSON response back to the guest.
pub(crate) fn write_into_guest(
    instance: &Instance,
    store: &mut Store<HostContext>,
    bytes: &[u8],
) -> AppResult<(i32, i32)> {
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut *store, "alloc")
        .map_err(|_| AppError::Internal("plugin does not export 'alloc'".to_string()))?;
    let ptr = alloc
        .call(&mut *store, bytes.len() as i32)
        .map_err(|e| AppError::Internal(format!("plugin 'alloc' trapped: {e}")))?;
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| AppError::Internal("plugin does not export 'memory'".to_string()))?;
    memory
        .write(&mut *store, ptr as usize, bytes)
        .map_err(|e| AppError::Internal(format!("failed writing into guest memory: {e}")))?;
    Ok((ptr, bytes.len() as i32))
}

fn read_from_guest(
    instance: &Instance,
    store: &mut Store<HostContext>,
    ptr: i32,
    len: i32,
) -> AppResult<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return Err(AppError::Internal(
            "plugin returned a negative pointer/length".to_string(),
        ));
    }
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| AppError::Internal("plugin does not export 'memory'".to_string()))?;
    memory
        .data(&mut *store)
        .get(ptr as usize..(ptr as usize + len as usize))
        .map(|s| s.to_vec())
        .ok_or_else(|| AppError::Internal("plugin returned an out-of-bounds buffer".to_string()))
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
