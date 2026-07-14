//! The "Alerts" half of Module 10: a rule engine subscribing to
//! `ListingEvent` + daily rollups (`docs/DESIGN.md` §6), dispatching to
//! `NotificationSink` implementations. Split into two independent
//! background loops — one per signal source, matching
//! `domain::alerts::AlertSignal`'s two variants:
//! - [`spawn`]: reacts to each live `ListingEvent` (price_drop, spread_widen,
//!   new_buyer, new_seller).
//! - [`spawn_daily_rollup_check`]: periodically re-checks `price_daily`
//!   (hist_low, hist_high) — these have no single triggering event.
//!
//! Deviation from `docs/DESIGN.md` §4's folder sketch (`infra/notify/sound.rs`):
//! the "sound" channel is *not* dispatched here — it plays client-side (Web
//! Audio, in `src/features/alerts`) off the pushed [`AlertFired`] event's
//! `channels` field. The window is already open and can do this with zero
//! new dependencies or platform audio quirks; a Rust-side audio-decode
//! dependency for a short beep wasn't worth it. `desktop`/`discord` are
//! real Rust-side sinks (`infra::notify::{os,discord}`), best-effort — a
//! failed sink logs a warning rather than aborting the loop, matching
//! `market_data_service.rs::persist_listing_event`'s established pattern.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::broadcast;

use crate::domain::alerts::{self, AlertKind, AlertSignal};
use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::domain::pricing::{self, Intent, Listing};
use crate::error::AppResult;
use crate::infra::backpack_tf::models::{ListingEvent, ListingEventKind};
use crate::infra::db::repos::alerts_repo::{
    AlertEventsRepo, AlertRuleRow, AlertRulesRepo, InsertAlertEvent,
};
use crate::infra::db::repos::items_repo::ItemsRepo;
use crate::infra::db::repos::market_listings_repo::MarketListingsRepo;
use crate::infra::db::repos::price_history_repo::PriceDailyRepo;
use crate::infra::keychain::{keys, Keychain};
use crate::infra::notify;
use crate::services::market_data_service::MarketDataService;

const DAY_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, Serialize, Type, Event)]
pub struct AlertFired {
    pub rule_id: i32,
    pub item_name: String,
    pub kind: String,
    pub message: String,
    pub fired_ts: f64,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct FiredAlert {
    rule_id: i32,
    item_name: String,
    kind: AlertKind,
    message: String,
    channels: Vec<String>,
}

/// Pure: does `rule` fire against `signal`, and if so, what should it say?
/// No I/O — trivially unit-testable, unlike the loops that call it.
fn check_rule(rule: &AlertRuleRow, item_name: &str, signal: &AlertSignal) -> Option<FiredAlert> {
    let kind = AlertKind::parse(&rule.kind).ok()?;
    if !alerts::evaluate(kind, rule.threshold, signal) {
        return None;
    }
    let channels: Vec<String> = serde_json::from_str(&rule.channels).unwrap_or_default();
    Some(FiredAlert {
        rule_id: rule.id,
        item_name: item_name.to_string(),
        kind,
        message: alerts::describe(kind, item_name, rule.threshold, signal),
        channels,
    })
}

/// Resolves `event` to an `ItemKey`/name, fetches its enabled rules, and
/// checks each. DB-only (no `AppHandle`), so it's testable against a
/// seeded pool without a live Tauri app. Returns `Ok(vec![])` for any of
/// the many legitimate "nothing to do" cases (removal event, no price,
/// unresolvable key, no rules for this item) rather than treating them as
/// errors.
async fn find_fired_alerts_for_event(
    pool: &SqlitePool,
    event: &ListingEvent,
) -> AppResult<Vec<FiredAlert>> {
    if event.kind == ListingEventKind::Removed {
        return Ok(Vec::new());
    }
    let Some(price_ref) = event.value_ref else {
        return Ok(Vec::new());
    };
    let Ok(quality) = Quality::try_from(event.quality) else {
        return Ok(Vec::new());
    };
    let Ok(killstreak_tier) = KillstreakTier::try_from(event.killstreak_tier) else {
        return Ok(Vec::new());
    };
    let key = ItemKey {
        defindex: event.defindex,
        quality,
        effect_id: event.effect_id,
        killstreak_tier,
        australium: event.australium,
        festivized: event.festivized,
        craftable: event.craftable,
    };

    let Some(item_id) = ItemsRepo::find_id_by_key(pool, &key).await? else {
        return Ok(Vec::new());
    };
    let rules = AlertRulesRepo::list_enabled_for_item(pool, item_id).await?;
    if rules.is_empty() {
        return Ok(Vec::new());
    }

    // Only bother computing the current spread if some rule actually
    // needs it — it costs an extra query.
    let needs_spread = rules
        .iter()
        .any(|r| r.kind == AlertKind::SpreadWiden.as_str());
    let current_spread_pct = if needs_spread {
        let rows = MarketListingsRepo::list_for_item_key(pool, &key).await?;
        let listings: Vec<Listing> = rows
            .iter()
            .map(|r| Listing {
                intent: if r.intent == "buy" {
                    Intent::Buy
                } else {
                    Intent::Sell
                },
                price_ref: r.price_ref,
            })
            .collect();
        pricing::spread(&listings).map(|s| s.pct)
    } else {
        None
    };

    let signal = AlertSignal::Listing {
        is_new: event.kind == ListingEventKind::New,
        intent: if event.intent == "buy" {
            Intent::Buy
        } else {
            Intent::Sell
        },
        price_ref,
        current_spread_pct,
    };

    let item_name = ItemsRepo::find_name_by_defindex(pool, key.defindex)
        .await?
        .unwrap_or_else(|| format!("Item {}", key.defindex));

    Ok(rules
        .iter()
        .filter_map(|rule| check_rule(rule, &item_name, &signal))
        .collect())
}

/// The rollup counterpart to [`find_fired_alerts_for_event`]: every
/// enabled `hist_low`/`hist_high` rule, checked against that item's
/// `price_daily` history (today's close vs. every prior day's close).
/// Skips a rule if today has no bar yet, or there's no prior history to
/// compare against — same DB-only, directly-testable shape.
async fn find_fired_hist_alerts(pool: &SqlitePool, now_ts: i64) -> AppResult<Vec<FiredAlert>> {
    let mut rules = AlertRulesRepo::list_enabled_by_kind(pool, AlertKind::HistLow.as_str()).await?;
    rules.extend(AlertRulesRepo::list_enabled_by_kind(pool, AlertKind::HistHigh.as_str()).await?);
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let today = now_ts / DAY_SECONDS;

    let mut fired = Vec::new();
    for rule in &rules {
        let history = PriceDailyRepo::history_since(pool, rule.item_id, 0).await?;

        let mut today_close = None;
        let mut prior_closes = Vec::new();
        for bar in &history {
            let Some(close) = bar.close_ref else { continue };
            if bar.day == today {
                today_close = Some(close);
            } else if bar.day < today {
                prior_closes.push(close);
            }
        }
        let Some(close_ref) = today_close else {
            continue;
        };
        if prior_closes.is_empty() {
            continue;
        }

        let signal = AlertSignal::DailyClose {
            close_ref,
            historical_low_ref: prior_closes.iter().cloned().fold(None, min_option),
            historical_high_ref: prior_closes.iter().cloned().fold(None, max_option),
        };

        let item_name = ItemsRepo::find_by_id(pool, rule.item_id)
            .await?
            .map(|row| row.name)
            .unwrap_or_else(|| format!("Item #{}", rule.item_id));

        if let Some(alert) = check_rule(rule, &item_name, &signal) {
            fired.push(alert);
        }
    }
    Ok(fired)
}

fn min_option(acc: Option<f64>, value: f64) -> Option<f64> {
    Some(acc.map_or(value, |a| a.min(value)))
}

fn max_option(acc: Option<f64>, value: f64) -> Option<f64> {
    Some(acc.map_or(value, |a| a.max(value)))
}

/// Records the fired alert, dispatches to its configured Rust-side sinks,
/// and pushes [`AlertFired`] to the frontend. Needs a live `AppHandle`
/// (for `desktop` notifications and the event emit), so unlike the
/// `find_fired_*` functions above, this isn't unit-tested directly — same
/// precedent as `market_data_service.rs`'s spawned loops.
async fn record_and_dispatch(
    app: &AppHandle,
    db: &SqlitePool,
    http: &reqwest::Client,
    fired: FiredAlert,
    fired_ts: i64,
) {
    let payload_json = serde_json::json!({
        "item_name": fired.item_name,
        "kind": fired.kind.as_str(),
        "message": fired.message,
    })
    .to_string();

    if let Err(e) = AlertEventsRepo::insert(
        db,
        &InsertAlertEvent {
            rule_id: fired.rule_id,
            fired_ts,
            payload_json: &payload_json,
        },
    )
    .await
    {
        tracing::warn!(error = %e, rule_id = fired.rule_id, "failed to record alert event");
    }

    for channel in &fired.channels {
        match channel.as_str() {
            "desktop" => {
                if let Err(e) = notify::os::send(app, &fired.item_name, &fired.message) {
                    tracing::warn!(error = %e, rule_id = fired.rule_id, "desktop notification failed");
                }
            }
            "discord" => match Keychain::get(keys::DISCORD_WEBHOOK_URL) {
                Ok(Some(url)) => {
                    if let Err(e) = notify::discord::send(http, &url, &fired.message).await {
                        tracing::warn!(error = %e, rule_id = fired.rule_id, "discord notification failed");
                    }
                }
                Ok(None) => {
                    tracing::debug!(
                        rule_id = fired.rule_id,
                        "discord channel enabled but no webhook URL configured"
                    )
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to read discord webhook URL from keychain")
                }
            },
            "sound" => {} // client-side — see the module-level deviation note.
            other => tracing::debug!(channel = other, "unknown alert channel, ignoring"),
        }
    }

    let _ = AlertFired {
        rule_id: fired.rule_id,
        item_name: fired.item_name,
        kind: fired.kind.as_str().to_string(),
        message: fired.message,
        fired_ts: fired_ts as f64,
        channels: fired.channels,
    }
    .emit(app);
}

/// Spawns the event-driven rule loop for the process's lifetime.
///
/// Uses `tauri::async_runtime::spawn`, not `tokio::spawn` — see the note
/// in `live_feed::spawn_relay`; both are called from `.setup()`, which
/// doesn't run inside an entered Tokio runtime on the calling thread.
pub fn spawn(app: AppHandle, db: SqlitePool, market_data: Arc<MarketDataService>) {
    let http = reqwest::Client::new();
    let mut events = market_data.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "alert service lagged behind the broadcast channel");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };

            let fired_ts = now_unix();
            match find_fired_alerts_for_event(&db, &event).await {
                Ok(fired_alerts) => {
                    for fired in fired_alerts {
                        record_and_dispatch(&app, &db, &http, fired, fired_ts).await;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to evaluate alert rules for listing event")
                }
            }
        }
    });
}

/// Spawns the periodic hist_low/hist_high sweep for the process's
/// lifetime, checking every `interval`.
pub fn spawn_daily_rollup_check(app: AppHandle, db: SqlitePool, interval: Duration) {
    let http = reqwest::Client::new();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let fired_ts = now_unix();
            match find_fired_hist_alerts(&db, fired_ts).await {
                Ok(fired_alerts) => {
                    for fired in fired_alerts {
                        record_and_dispatch(&app, &db, &http, fired, fired_ts).await;
                    }
                }
                Err(e) => tracing::warn!(error = %e, "daily rollup alert check failed"),
            }
        }
    });
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
#[path = "alert_service_tests.rs"]
mod tests;
