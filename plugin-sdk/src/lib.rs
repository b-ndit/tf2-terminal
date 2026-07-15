//! Safe Rust SDK for writing a TF2 Terminal plugin (Module 14, see
//! `docs/DESIGN.md` §8). A plugin is a `wasm32-wasip1` `cdylib` with a
//! `plugin.toml` manifest declaring its name/version/capabilities/events;
//! see `sample-plugin/` at the repo root for a complete working example.
//!
//! ## Getting started
//!
//! ```ignore
//! tf2_terminal_plugin_sdk::export_abi!();
//!
//! #[no_mangle]
//! pub extern "C" fn plugin_init(in_ptr: i32, in_len: i32) -> i64 {
//!     let _ = tf2_terminal_plugin_sdk::read_input(in_ptr, in_len);
//!     tf2_terminal_plugin_sdk::log(tf2_terminal_plugin_sdk::LogLevel::Info, "plugin started");
//!     tf2_terminal_plugin_sdk::pack_output(&[])
//! }
//! ```
//!
//! `export_abi!()` generates the mandatory `alloc`/`dealloc` exports.
//! Every host-called hook (`plugin_init`, `on_alert_fired`,
//! `provide_listings` — all optional, implement only what your
//! `plugin.toml` declares in `events`) has the fixed signature `(in_ptr:
//! i32, in_len: i32) -> i64`; use [`read_input`] to decode the input and
//! [`pack_output`] to encode the return value.

pub mod abi;
mod host;

pub use abi::{pack_output, read_input};
pub use host::{
    http_fetch, inventory_list, log, market_price_daily, notify_send, HostError, InventoryItem,
    LogLevel, PriceDaily,
};
