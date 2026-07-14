//! Relays the live backpack.tf listing stream straight to the frontend —
//! the "Live Feed" half of Module 10 (`docs/DESIGN.md` §9's mockup: a
//! continuously scrolling panel of new sells/delists/buyers). No DB, no
//! rules — that's `alert_service`'s job; this is a pure fan-out off
//! `MarketDataService`'s broadcast channel (`docs/DESIGN.md` §6, unused
//! since Module 5/7 — "no subscriber exists yet").

use std::sync::Arc;

use serde::Serialize;
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::infra::backpack_tf::models::ListingEvent;
use crate::services::market_data_service::MarketDataService;

/// Tuple wrapper adding the `Event` derive at the services layer, so
/// `infra/backpack_tf/models.rs::ListingEvent` itself stays Tauri-agnostic
/// (nothing in `infra/` imports `tauri` today).
#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct ListingEventPushed(pub ListingEvent);

/// Spawns the relay for the process's lifetime. Must run after Tauri's
/// `AppHandle` exists (`lib.rs`'s `.setup()` closure), unlike
/// `MarketDataService::spawn_listener` itself, which starts earlier in
/// `app::build()` and doesn't need one.
pub fn spawn_relay(app: AppHandle, market_data: Arc<MarketDataService>) {
    let mut events = market_data.subscribe();
    // `tauri::async_runtime::spawn`, not `tokio::spawn`: this is called
    // from `lib.rs`'s `.setup()` closure, which — unlike `app::build()`'s
    // `tauri::async_runtime::block_on` context — doesn't run inside an
    // entered Tokio runtime on the calling thread; a raw `tokio::spawn`
    // there panics ("there is no reactor running"). Tauri's wrapper
    // dispatches onto its managed runtime regardless of caller context.
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if let Err(e) = ListingEventPushed(event).emit(&app) {
                        tracing::warn!(error = %e, "failed to emit live feed event");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        skipped,
                        "live feed relay lagged behind the broadcast channel"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
