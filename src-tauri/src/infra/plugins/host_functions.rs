//! The five capability-gated `host_*` imports every plugin links against
//! (module namespace `"host"`), plus the WASI preview1 shim. Every
//! function follows the same shape: check the capability, check the rate
//! limiter, do the work (blocking on the plugin's own tokio handle since
//! wasmtime host functions are synchronous), write the JSON response back
//! into guest memory via [`super::runtime::write_into_guest`], and return
//! its length — or a negative error code (`-1` capability denied, `-2`
//! rate limited, `-3` internal).

use serde::Serialize;
use wasmtime::{Caller, Linker};
use wasmtime_wasi::p1;

use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::domain::plugin::Capability;
use crate::infra::db::repos::inventory_repo::InventoryRepo;
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::price_history_repo::PriceDailyRepo;
use crate::infra::keychain::{keys, Keychain};
use crate::infra::notify;
use crate::infra::plugins::runtime::HostContext;

const CAP_DENIED: i32 = -1;
const RATE_LIMITED: i32 = -2;
const INTERNAL_ERROR: i32 = -3;

pub fn register(linker: &mut Linker<HostContext>) -> wasmtime::Result<()> {
    p1::add_to_linker_sync(linker, |ctx: &mut HostContext| &mut ctx.wasi)?;

    linker.func_wrap("host", "host_log", host_log)?;
    linker.func_wrap("host", "host_market_price_daily", host_market_price_daily)?;
    linker.func_wrap("host", "host_inventory_list", host_inventory_list)?;
    linker.func_wrap("host", "host_http_fetch", host_http_fetch)?;
    linker.func_wrap("host", "host_notify_send", host_notify_send)?;
    Ok(())
}

fn has_capability(ctx: &HostContext, matches: impl Fn(&Capability) -> bool) -> bool {
    ctx.granted.iter().any(matches)
}

fn read_guest_bytes(caller: &mut Caller<'_, HostContext>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let memory = caller.get_export("memory")?.into_memory()?;
    memory
        .data(&mut *caller)
        .get(ptr as usize..(ptr as usize + len as usize))
        .map(|s| s.to_vec())
}

fn write_json_response<T: Serialize>(
    caller: &mut Caller<'_, HostContext>,
    out_ptr_ptr: i32,
    value: &T,
) -> i32 {
    let Ok(bytes) = serde_json::to_vec(value) else {
        return INTERNAL_ERROR;
    };
    write_bytes_response(caller, out_ptr_ptr, &bytes)
}

fn write_bytes_response(
    caller: &mut Caller<'_, HostContext>,
    out_ptr_ptr: i32,
    bytes: &[u8],
) -> i32 {
    if bytes.is_empty() {
        return 0;
    }
    // `write_into_guest` (used by `PluginRuntime::call_export_json`)
    // operates on an `Instance`/`Store`, but inside a host function we
    // only have a `Caller` — call the guest's `alloc`/`memory` exports
    // directly through it, then relay the pointer into the
    // guest-provided `out_ptr_ptr` cell, same convention either way.
    let Some(alloc_func) = caller.get_export("alloc").and_then(|e| e.into_func()) else {
        return INTERNAL_ERROR;
    };
    let Ok(alloc) = alloc_func.typed::<i32, i32>(&caller) else {
        return INTERNAL_ERROR;
    };
    let Ok(ptr) = alloc.call(&mut *caller, bytes.len() as i32) else {
        return INTERNAL_ERROR;
    };
    let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
        return INTERNAL_ERROR;
    };
    if memory.write(&mut *caller, ptr as usize, bytes).is_err() {
        return INTERNAL_ERROR;
    }
    if memory
        .write(&mut *caller, out_ptr_ptr as usize, &ptr.to_le_bytes())
        .is_err()
    {
        return INTERNAL_ERROR;
    }
    bytes.len() as i32
}

/// `host_log(level: i32, ptr: i32, len: i32)` — no capability required
/// (diagnostic only, never reaches the user's OS/network), still subject
/// to the rate limiter so a misbehaving plugin can't flood the log file.
fn host_log(mut caller: Caller<'_, HostContext>, level: i32, ptr: i32, len: i32) {
    if caller.data().limiter.check().is_err() {
        return;
    }
    let Some(bytes) = read_guest_bytes(&mut caller, ptr, len) else {
        return;
    };
    let message = String::from_utf8_lossy(&bytes);
    let plugin_name = caller.data().plugin_name.clone();
    match level {
        0 => tracing::debug!(plugin = %plugin_name, "{message}"),
        2 => tracing::warn!(plugin = %plugin_name, "{message}"),
        3 => tracing::error!(plugin = %plugin_name, "{message}"),
        _ => tracing::info!(plugin = %plugin_name, "{message}"),
    }
}

#[derive(Serialize)]
struct PriceDailyDto {
    day: i64,
    open_ref: Option<f64>,
    high_ref: Option<f64>,
    low_ref: Option<f64>,
    close_ref: Option<f64>,
}

/// `host_market_price_daily(defindex: i32, quality: i32, out_ptr_ptr: i32)
/// -> i32` — the most recent `price_daily` bar for the simplified item key
/// `(defindex, quality)` (no killstreak/effect/australium variant — a
/// deliberately reduced plugin-facing subset, not the full `ItemKey`).
/// Requires [`Capability::MarketRead`].
fn host_market_price_daily(
    mut caller: Caller<'_, HostContext>,
    defindex: i32,
    quality: i32,
    out_ptr_ptr: i32,
) -> i32 {
    if !has_capability(caller.data(), |c| matches!(c, Capability::MarketRead)) {
        return CAP_DENIED;
    }
    if caller.data().limiter.check().is_err() {
        return RATE_LIMITED;
    }
    let Ok(quality) = Quality::try_from(quality.clamp(0, u8::MAX as i32) as u8) else {
        return 0;
    };
    let key = ItemKey {
        defindex: defindex.max(0) as u32,
        quality,
        effect_id: None,
        killstreak_tier: KillstreakTier::None,
        australium: false,
        festivized: false,
        craftable: true,
    };

    let pool = caller.data().db.clone();
    let handle = caller.data().tokio_handle.clone();
    let dto = handle.block_on(async move {
        let Ok(Some(item_id)) = ItemsRepo::find_id_by_key(&pool, &key).await else {
            return None;
        };
        let Ok(history) = PriceDailyRepo::history_since(&pool, item_id, 0).await else {
            return None;
        };
        history.last().map(|bar| PriceDailyDto {
            day: bar.day,
            open_ref: bar.open_ref,
            high_ref: bar.high_ref,
            low_ref: bar.low_ref,
            close_ref: bar.close_ref,
        })
    });

    match dto {
        Some(dto) => write_json_response(&mut caller, out_ptr_ptr, &dto),
        None => 0,
    }
}

#[derive(Serialize)]
struct InventoryItemDto {
    name: String,
    quality: i64,
    defindex: i64,
}

/// `host_inventory_list(out_ptr_ptr: i32) -> i32` — the logged-in user's
/// synced inventory, as a reduced `{name, quality, defindex}` list.
/// Requires [`Capability::InventoryRead`]; `-3` if nobody is logged in.
fn host_inventory_list(mut caller: Caller<'_, HostContext>, out_ptr_ptr: i32) -> i32 {
    if !has_capability(caller.data(), |c| matches!(c, Capability::InventoryRead)) {
        return CAP_DENIED;
    }
    if caller.data().limiter.check().is_err() {
        return RATE_LIMITED;
    }
    let Some(steam_id) = caller.data().steam_id.clone() else {
        return INTERNAL_ERROR;
    };

    let pool = caller.data().db.clone();
    let handle = caller.data().tokio_handle.clone();
    let items =
        handle.block_on(async move { InventoryRepo::list_with_items(&pool, &steam_id).await });

    match items {
        Ok(items) => {
            let dto: Vec<InventoryItemDto> = items
                .into_iter()
                .map(|i| InventoryItemDto {
                    name: i.name,
                    quality: i.quality,
                    defindex: i.defindex,
                })
                .collect();
            write_json_response(&mut caller, out_ptr_ptr, &dto)
        }
        Err(_) => INTERNAL_ERROR,
    }
}

/// `host_http_fetch(url_ptr: i32, url_len: i32, out_ptr_ptr: i32) -> i32`
/// — a raw GET, response body written back as-is (not JSON-wrapped).
/// Requires an [`Capability::Http`] entry whose domain list contains the
/// URL's host exactly.
fn host_http_fetch(
    mut caller: Caller<'_, HostContext>,
    url_ptr: i32,
    url_len: i32,
    out_ptr_ptr: i32,
) -> i32 {
    let Some(url_bytes) = read_guest_bytes(&mut caller, url_ptr, url_len) else {
        return INTERNAL_ERROR;
    };
    let Ok(url_str) = String::from_utf8(url_bytes) else {
        return INTERNAL_ERROR;
    };
    let Ok(parsed) = url::Url::parse(&url_str) else {
        return INTERNAL_ERROR;
    };
    let Some(host) = parsed.host_str() else {
        return INTERNAL_ERROR;
    };
    if !has_capability(caller.data(), |c| c.allows_http_host(host)) {
        return CAP_DENIED;
    }
    if caller.data().limiter.check().is_err() {
        return RATE_LIMITED;
    }

    let http = caller.data().http.clone();
    let handle = caller.data().tokio_handle.clone();
    let body = handle.block_on(async move {
        let response = http.get(url_str).send().await.ok()?;
        response.bytes().await.ok()
    });

    match body {
        Some(bytes) => write_bytes_response(&mut caller, out_ptr_ptr, &bytes),
        None => INTERNAL_ERROR,
    }
}

/// `host_notify_send(title_ptr, title_len, body_ptr, body_len) -> i32` —
/// posts to the user's configured Discord webhook (the only real sink
/// reachable without a live `AppHandle`; see the Module 14 implementation
/// note in `docs/DESIGN.md`). Best-effort: a missing webhook or a failed
/// POST both return `0`, matching `alert_service`'s own "log and
/// continue" sink semantics. Requires [`Capability::NotifySend`].
fn host_notify_send(
    mut caller: Caller<'_, HostContext>,
    title_ptr: i32,
    title_len: i32,
    body_ptr: i32,
    body_len: i32,
) -> i32 {
    if !has_capability(caller.data(), |c| matches!(c, Capability::NotifySend)) {
        return CAP_DENIED;
    }
    if caller.data().limiter.check().is_err() {
        return RATE_LIMITED;
    }
    let Some(title_bytes) = read_guest_bytes(&mut caller, title_ptr, title_len) else {
        return INTERNAL_ERROR;
    };
    let Some(body_bytes) = read_guest_bytes(&mut caller, body_ptr, body_len) else {
        return INTERNAL_ERROR;
    };
    let title = String::from_utf8_lossy(&title_bytes).into_owned();
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    let plugin_name = caller.data().plugin_name.clone();

    let http = caller.data().http.clone();
    let handle = caller.data().tokio_handle.clone();
    handle.block_on(async move {
        match Keychain::get(keys::DISCORD_WEBHOOK_URL) {
            Ok(Some(webhook_url)) => {
                let content = format!("[plugin: {plugin_name}] {title}: {body}");
                if let Err(e) = notify::discord::send(&http, &webhook_url, &content).await {
                    tracing::warn!(error = %e, plugin = %plugin_name, "plugin notification failed");
                }
            }
            Ok(None) => {
                tracing::warn!(plugin = %plugin_name, "plugin sent a notification but no discord webhook is configured");
            }
            Err(e) => {
                tracing::warn!(error = %e, plugin = %plugin_name, "failed to read discord webhook url from keychain");
            }
        }
    });
    0
}

#[cfg(test)]
#[path = "host_functions_tests.rs"]
mod tests;
