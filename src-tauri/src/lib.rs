mod app;
mod commands;
mod domain;
mod error;
mod infra;
mod services;
mod telemetry;

use std::time::Duration;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder};

/// How often the hist_low/hist_high sweep re-checks `price_daily` — these
/// react to a day closing, not a single event, so there's no benefit to
/// checking faster than that; hourly is frequent enough to catch a new
/// extreme without much wasted work.
const HIST_ROLLUP_CHECK_INTERVAL: Duration = Duration::from_secs(3600);

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::settings::health_check,
            commands::settings::get_config,
            commands::settings::set_steam_api_key,
            commands::settings::has_steam_api_key,
            commands::settings::clear_steam_api_key,
            commands::settings::set_backpack_tf_token,
            commands::settings::has_backpack_tf_token,
            commands::settings::clear_backpack_tf_token,
            commands::settings::set_discord_webhook_url,
            commands::settings::has_discord_webhook_url,
            commands::settings::clear_discord_webhook_url,
            commands::schema::sync_item_schema,
            commands::auth::login_with_steam,
            commands::auth::get_steam_id,
            commands::auth::logout_steam,
            commands::inventory::sync_inventory,
            commands::inventory::get_inventory,
            commands::organization::set_favorite,
            commands::organization::set_pinned,
            commands::organization::set_folder,
            commands::organization::set_note,
            commands::organization::set_custom_label,
            commands::organization::list_tags,
            commands::organization::create_tag,
            commands::organization::delete_tag,
            commands::organization::add_item_tag,
            commands::organization::remove_item_tag,
            commands::market::sync_price_catalog,
            commands::market::get_recent_listings,
            commands::market::analyze_classified_url,
            commands::market::get_price_history,
            commands::trades::get_active_trades,
            commands::alerts::create_alert_rule,
            commands::alerts::list_alert_rules,
            commands::alerts::set_alert_rule_enabled,
            commands::alerts::delete_alert_rule,
            commands::alerts::list_recent_alert_events,
            commands::alerts::ack_alert_event,
            commands::flip_finder::get_flip_opportunities,
            commands::flip_finder::add_to_watchlist,
            commands::flip_finder::remove_from_watchlist,
            commands::flip_finder::list_watchlist,
        ])
        .events(collect_events![
            commands::inventory::InventoryChanged,
            services::live_feed::ListingEventPushed,
            services::alert_service::AlertFired,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta_builder = specta_builder();

    #[cfg(debug_assertions)]
    {
        // CARGO_MANIFEST_DIR is a compile-time constant (src-tauri/), so this
        // export lands in the right place regardless of the process cwd.
        let bindings_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
        specta_builder
            .export(specta_typescript::Typescript::default(), bindings_path)
            .expect("failed to export typescript bindings");
    }

    let app_state = tauri::async_runtime::block_on(app::build())
        .expect("failed to initialize application state");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(specta_builder.invoke_handler())
        .manage(app_state)
        .setup(move |app| {
            specta_builder.mount_events(app);

            // Only reachable here, not from `app::build()`: these need a
            // live `AppHandle` to emit events / show OS notifications,
            // which doesn't exist until Tauri's `App` is constructed.
            let handle = app.handle().clone();
            let state = app.state::<app::AppState>();
            services::live_feed::spawn_relay(handle.clone(), state.market_data.clone());
            services::alert_service::spawn(
                handle.clone(),
                state.db.clone(),
                state.market_data.clone(),
            );
            services::alert_service::spawn_daily_rollup_check(
                handle,
                state.db.clone(),
                HIST_ROLLUP_CHECK_INTERVAL,
            );

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for the recurring Specta/TypeScript export bug
    /// (`i64`/`u64`/`usize` fields panic at export time): exercises the same
    /// `Builder::export` call `run()` makes, without needing a full Tauri
    /// app or display.
    #[test]
    fn typescript_bindings_export_without_panicking() {
        let dir = std::env::temp_dir().join(format!(
            "tf2-terminal-bindings-export-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let bindings_path = dir.join("bindings.ts");

        specta_builder()
            .export(specta_typescript::Typescript::default(), &bindings_path)
            .expect("typescript bindings export should not panic");

        std::fs::remove_dir_all(&dir).ok();
    }
}
