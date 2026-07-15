//! Example TF2 Terminal plugin (Module 14). Demonstrates the three pieces
//! a real plugin touches: the mandatory ABI exports (via
//! [`tf2_terminal_plugin_sdk::export_abi`]), the optional `plugin_init`
//! hook, and `on_alert_fired` — subscribed via this crate's `plugin.toml`
//! (`events = ["alert_fired"]`), using both capabilities it requests
//! there (`market:read`, `notify:send`).

use serde::Deserialize;
use tf2_terminal_plugin_sdk as sdk;

sdk::export_abi!();

/// Mirrors `alert_service::AlertFired` on the host — a deliberately
/// reduced, plugin-facing shape (this crate only reads what it uses).
#[derive(Deserialize)]
struct AlertFiredPayload {
    rule_id: i32,
    item_name: String,
    kind: String,
    message: String,
}

#[no_mangle]
pub extern "C" fn plugin_init(in_ptr: i32, in_len: i32) -> i64 {
    let _ = sdk::read_input(in_ptr, in_len);
    sdk::log(sdk::LogLevel::Info, "sample-plugin initialized");
    sdk::pack_output(&[])
}

/// Demonstrates the `market_provider` event (`docs/DESIGN.md` §8): a
/// fixed, deterministic listing, shaped exactly like the host's
/// `ListingEvent` (field-for-field, though this crate deliberately
/// doesn't depend on the backend to construct it — a real external
/// plugin wouldn't have access to that type either). The host injects
/// whatever this returns into the same broadcast bus every other market
/// data source feeds.
#[no_mangle]
pub extern "C" fn provide_listings(in_ptr: i32, in_len: i32) -> i64 {
    let _ = sdk::read_input(in_ptr, in_len);
    let listings = serde_json::json!([{
        "listing_id": "sample-plugin:demo-1",
        "kind": "new",
        "defindex": 5021,
        "quality": 6,
        "effect_id": null,
        "killstreak_tier": 0,
        "australium": false,
        "festivized": false,
        "craftable": true,
        "intent": "sell",
        "steam_id": "0",
        "steam_name": "sample-plugin",
        "value_ref": 62.0,
    }]);
    let bytes = serde_json::to_vec(&listings).unwrap_or_default();
    sdk::pack_output(&bytes)
}

#[no_mangle]
pub extern "C" fn on_alert_fired(in_ptr: i32, in_len: i32) -> i64 {
    let bytes = sdk::read_input(in_ptr, in_len);
    let Ok(payload) = serde_json::from_slice::<AlertFiredPayload>(&bytes) else {
        sdk::log(sdk::LogLevel::Warn, "failed to parse alert payload");
        return sdk::pack_output(&[]);
    };

    // Demonstrate `market:read`: this alert payload doesn't carry a
    // defindex, so this checks a fixed reference item (a key, defindex
    // 5021) purely to show the capability working end to end.
    match sdk::market_price_daily(5021, 6) {
        Ok(Some(price)) => sdk::log(
            sdk::LogLevel::Info,
            &format!("key price context: close_ref={:?}", price.close_ref),
        ),
        Ok(None) => sdk::log(sdk::LogLevel::Debug, "no key price data available yet"),
        Err(e) => sdk::log(sdk::LogLevel::Warn, &format!("price lookup failed: {e:?}")),
    }

    // Demonstrate `notify:send`: relay the alert through the Discord
    // webhook sink, prefixed so it's clear this came from a plugin.
    let title = format!("Alert #{}: {}", payload.rule_id, payload.kind);
    let body = format!("{}: {}", payload.item_name, payload.message);
    if let Err(e) = sdk::notify_send(&title, &body) {
        sdk::log(sdk::LogLevel::Warn, &format!("notify_send failed: {e:?}"));
    }

    sdk::pack_output(&[])
}
