//! Safe wrappers over the five `host_*` imports (module namespace
//! `"host"`) — see `src-tauri/src/infra/plugins/host_functions.rs` for the
//! host-side implementation these calls reach. Every function here
//! follows the same shape: call the raw import, decode its status code
//! into a [`HostError`] on failure, otherwise (for calls that return data)
//! read the response out of this plugin's own memory at the pointer the
//! host wrote back.

use serde::Deserialize;

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    fn host_log(level: i32, ptr: i32, len: i32);
    fn host_market_price_daily(defindex: i32, quality: i32, out_ptr_ptr: i32) -> i32;
    fn host_inventory_list(out_ptr_ptr: i32) -> i32;
    fn host_http_fetch(url_ptr: i32, url_len: i32, out_ptr_ptr: i32) -> i32;
    fn host_notify_send(title_ptr: i32, title_len: i32, body_ptr: i32, body_len: i32) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_i32(self) -> i32 {
        match self {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warn => 2,
            LogLevel::Error => 3,
        }
    }
}

/// Mirrors the negative status codes every `host_*` import can return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostError {
    /// The plugin's manifest didn't request the capability this call needs.
    CapabilityDenied,
    /// Too many host calls too quickly — try again later.
    RateLimited,
    /// Anything else (a malformed argument, an internal host failure).
    Internal,
}

impl HostError {
    fn from_code(code: i32) -> Self {
        match code {
            -1 => HostError::CapabilityDenied,
            -2 => HostError::RateLimited,
            _ => HostError::Internal,
        }
    }
}

/// A deliberately reduced subset of `price_daily` — not a 1:1 mirror of
/// the backend's internal row type.
#[derive(Debug, Clone, Deserialize)]
pub struct PriceDaily {
    pub day: i64,
    pub open_ref: Option<f64>,
    pub high_ref: Option<f64>,
    pub low_ref: Option<f64>,
    pub close_ref: Option<f64>,
}

/// A deliberately reduced subset of an inventory item.
#[derive(Debug, Clone, Deserialize)]
pub struct InventoryItem {
    pub name: String,
    pub quality: i64,
    pub defindex: i64,
}

pub fn log(level: LogLevel, msg: &str) {
    unsafe { host_log(level.as_i32(), msg.as_ptr() as i32, msg.len() as i32) }
}

/// Calls a `host_*` import that writes its response through an
/// `out_ptr_ptr` cell, returning the decoded bytes it wrote (`None` for
/// the `0` "no data" sentinel).
fn call_with_output(f: impl FnOnce(i32) -> i32) -> Result<Option<Vec<u8>>, HostError> {
    let mut out_ptr_cell: i32 = 0;
    let out_ptr_ptr = &mut out_ptr_cell as *mut i32 as i32;
    let status = f(out_ptr_ptr);
    if status < 0 {
        return Err(HostError::from_code(status));
    }
    if status == 0 {
        return Ok(None);
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(out_ptr_cell as *const u8, status as usize).to_vec() };
    Ok(Some(bytes))
}

/// Requires `market:read`.
pub fn market_price_daily(defindex: u32, quality: u8) -> Result<Option<PriceDaily>, HostError> {
    let bytes = call_with_output(|out_ptr_ptr| unsafe {
        host_market_price_daily(defindex as i32, quality as i32, out_ptr_ptr)
    })?;
    match bytes {
        None => Ok(None),
        Some(b) => serde_json::from_slice(&b)
            .map(Some)
            .map_err(|_| HostError::Internal),
    }
}

/// Requires `inventory:read`; fails with [`HostError::Internal`] if
/// nobody is logged in.
pub fn inventory_list() -> Result<Vec<InventoryItem>, HostError> {
    let bytes = call_with_output(|out_ptr_ptr| unsafe { host_inventory_list(out_ptr_ptr) })?;
    match bytes {
        None => Ok(Vec::new()),
        Some(b) => serde_json::from_slice(&b).map_err(|_| HostError::Internal),
    }
}

/// Requires an `http:{domains}` capability listing `url`'s host exactly.
/// Returns the raw response body — not JSON-decoded, since this is a
/// generic passthrough fetch.
pub fn http_fetch(url: &str) -> Result<Vec<u8>, HostError> {
    let bytes = call_with_output(|out_ptr_ptr| unsafe {
        host_http_fetch(url.as_ptr() as i32, url.len() as i32, out_ptr_ptr)
    })?;
    Ok(bytes.unwrap_or_default())
}

/// Requires `notify:send`. Best-effort on the host side (a missing
/// webhook or a failed delivery both still return `Ok(())`) — this only
/// errors if the capability itself was never granted or the call was
/// rate-limited.
pub fn notify_send(title: &str, body: &str) -> Result<(), HostError> {
    let status = unsafe {
        host_notify_send(
            title.as_ptr() as i32,
            title.len() as i32,
            body.as_ptr() as i32,
            body.len() as i32,
        )
    };
    if status < 0 {
        return Err(HostError::from_code(status));
    }
    Ok(())
}
