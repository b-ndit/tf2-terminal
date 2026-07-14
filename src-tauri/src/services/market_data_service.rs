use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use specta::Type;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::error::{AppError, AppResult};
use crate::infra::backpack_tf::client::BackpackTfClient;
use crate::infra::backpack_tf::models::{ListingEvent, ListingEventKind, WsListingPayload};
use crate::infra::backpack_tf::websocket::{self, RawListingEvent};
use crate::infra::db::repos::kv_cache_repo::KvCacheRepo;
use crate::infra::db::repos::market_listings_repo::{MarketListingsRepo, UpsertMarketListing};
use crate::services::history_recorder::HistoryRecorder;

const RECENT_EVENTS_CAP: usize = 200;
const SEEN_LISTINGS_CAP: usize = 50_000;
const PRICE_CACHE_KEY: &str = "backpack_tf:price_catalog";
const PRICE_CACHE_TTL: Duration = Duration::from_secs(3600);

/// Bounded "have we seen this listing id before" tracker, so the websocket's
/// single `listing-update` event type can be split into `New` vs `Updated`
/// (`docs/DESIGN.md` §6's `ListingEvent.kind`). Capped with simple FIFO
/// eviction — the marketplace has far more listings over time than we need
/// to remember exactly.
struct SeenListings {
    set: HashSet<String>,
    order: VecDeque<String>,
    cap: usize,
}

impl SeenListings {
    fn new(cap: usize) -> Self {
        Self {
            set: HashSet::new(),
            order: VecDeque::new(),
            cap,
        }
    }

    /// Returns `true` the first time `id` is observed.
    fn observe(&mut self, id: &str) -> bool {
        if self.set.contains(id) {
            return false;
        }
        self.set.insert(id.to_string());
        self.order.push_back(id.to_string());
        if self.order.len() > self.cap {
            if let Some(oldest) = self.order.pop_front() {
                self.set.remove(&oldest);
            }
        }
        true
    }

    fn remove(&mut self, id: &str) {
        self.set.remove(id);
    }
}

#[derive(Debug, Serialize, Type)]
pub struct PriceCatalogSyncSummary {
    pub items_cached: u32,
    pub from_cache: bool,
}

/// Owns the backpack.tf REST client, websocket consumer, and (implicitly,
/// via the REST client) rate limiter. Emits [`ListingEvent`] on an internal
/// broadcast channel — single ingestion point, fan-out consumers (History
/// Recorder, Flip Finder, Alerts, ...), per `docs/DESIGN.md` §6.
pub struct MarketDataService {
    events_tx: broadcast::Sender<ListingEvent>,
    recent: Mutex<VecDeque<ListingEvent>>,
    api: BackpackTfClient,
}

impl MarketDataService {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(1024);
        Self {
            events_tx,
            recent: Mutex::new(VecDeque::with_capacity(RECENT_EVENTS_CAP)),
            api: BackpackTfClient::new(),
        }
    }

    // Consumed by Module 8 (History Recorder), Module 10 (Live Feed), and
    // Module 11 (Flip Finder) — "single ingestion point, fan-out consumers"
    // per docs/DESIGN.md §6. No subscriber exists yet.
    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<ListingEvent> {
        self.events_tx.subscribe()
    }

    pub async fn recent_events(&self) -> Vec<ListingEvent> {
        self.recent.lock().await.iter().cloned().collect()
    }

    /// Spawns the websocket consumer as a background task for the process's
    /// lifetime. No Steam login or API key required — this runs whenever
    /// the app is open. Also persists each event into `market_listings`
    /// (Module 7's own accumulated view of the market, since the classifieds
    /// snapshot endpoint was dropped in Module 5) so per-item buyers/sellers
    /// tables have something to query.
    pub fn spawn_listener(self: &Arc<Self>, pool: sqlx::SqlitePool) {
        let (raw_tx, mut raw_rx) = mpsc::channel::<RawListingEvent>(256);
        tokio::spawn(websocket::run_with_reconnect(raw_tx));

        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut seen = SeenListings::new(SEEN_LISTINGS_CAP);
            while let Some(raw) = raw_rx.recv().await {
                let event = match raw {
                    RawListingEvent::Upserted(payload) => {
                        let kind = if seen.observe(&payload.id) {
                            ListingEventKind::New
                        } else {
                            ListingEventKind::Updated
                        };
                        to_listing_event(payload, kind)
                    }
                    RawListingEvent::Deleted(payload) => {
                        seen.remove(&payload.id);
                        to_listing_event(payload, ListingEventKind::Removed)
                    }
                };

                persist_listing_event(&pool, &event).await;

                {
                    let mut recent = service.recent.lock().await;
                    if recent.len() >= RECENT_EVENTS_CAP {
                        recent.pop_front();
                    }
                    recent.push_back(event.clone());
                }
                let _ = service.events_tx.send(event);
            }
        });
    }

    /// Fetches (or reuses a cached, still-fresh copy of) the community
    /// price catalog. The API itself caches for 900s server-side; we cache
    /// the raw response for an hour to keep this cheap to call often.
    pub async fn sync_price_catalog(
        &self,
        pool: &sqlx::SqlitePool,
        api_key: &str,
    ) -> AppResult<PriceCatalogSyncSummary> {
        if let Some(cached) = KvCacheRepo::get(pool, PRICE_CACHE_KEY).await? {
            let count = cached_item_count(&cached)?;
            return Ok(PriceCatalogSyncSummary {
                items_cached: count,
                from_cache: true,
            });
        }

        let catalog = self.api.fetch_price_catalog(api_key).await?;
        let json = serde_json::to_vec(&catalog)
            .map_err(|e| AppError::Internal(format!("failed to serialize price catalog: {e}")))?;
        let count = catalog.items.len() as u32;
        KvCacheRepo::set(pool, PRICE_CACHE_KEY, &json, PRICE_CACHE_TTL).await?;

        if let Err(e) = HistoryRecorder::record_schema_sync(pool, &catalog).await {
            tracing::warn!(error = %e, "failed to record schema-sync price points");
        }

        Ok(PriceCatalogSyncSummary {
            items_cached: count,
            from_cache: false,
        })
    }
}

impl Default for MarketDataService {
    fn default() -> Self {
        Self::new()
    }
}

fn to_listing_event(payload: WsListingPayload, kind: ListingEventKind) -> ListingEvent {
    ListingEvent {
        listing_id: payload.id,
        kind,
        defindex: payload.item.defindex,
        quality: payload.item.quality.id,
        effect_id: payload.item.particle.as_ref().map(|p| p.id),
        killstreak_tier: payload.item.killstreak_tier.unwrap_or(0),
        australium: payload.item.australium.unwrap_or(false),
        festivized: payload.item.festivized.unwrap_or(false),
        craftable: payload.item.craftable.unwrap_or(true),
        intent: payload.intent,
        steam_id: payload.steamid,
        steam_name: payload.user.map(|u| u.name),
        value_ref: payload.value.map(|v| v.raw),
    }
}

/// Best-effort: a failed persist (e.g. a transient DB hiccup) shouldn't
/// interrupt the live event stream — the broadcast/recent-buffer path
/// above still carries the event either way.
async fn persist_listing_event(pool: &sqlx::SqlitePool, event: &ListingEvent) {
    let result = if event.kind == ListingEventKind::Removed {
        MarketListingsRepo::delete(pool, &event.listing_id).await
    } else {
        let Some(price_ref) = event.value_ref else {
            return;
        };
        MarketListingsRepo::upsert(
            pool,
            &UpsertMarketListing {
                listing_id: &event.listing_id,
                defindex: event.defindex as i64,
                quality: event.quality as i64,
                effect_id: event.effect_id.map(|e| e as i64),
                killstreak_tier: event.killstreak_tier as i64,
                australium: event.australium,
                festivized: event.festivized,
                craftable: event.craftable,
                intent: &event.intent,
                price_ref,
                steam_id: &event.steam_id,
                steam_name: event.steam_name.as_deref(),
                updated_at: now_unix(),
            },
        )
        .await
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, listing_id = %event.listing_id, "failed to persist market listing");
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

fn cached_item_count(raw: &[u8]) -> AppResult<u32> {
    let catalog: crate::infra::backpack_tf::models::PriceCatalogResponse =
        serde_json::from_slice(raw)
            .map_err(|e| AppError::Internal(format!("corrupt cached price catalog: {e}")))?;
    Ok(catalog.items.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_listings_distinguishes_first_sight_from_repeats() {
        let mut seen = SeenListings::new(10);
        assert!(seen.observe("a"));
        assert!(!seen.observe("a"));
        assert!(seen.observe("b"));
    }

    #[test]
    fn seen_listings_forgets_after_removal() {
        let mut seen = SeenListings::new(10);
        seen.observe("a");
        seen.remove("a");
        assert!(seen.observe("a"));
    }

    #[test]
    fn seen_listings_evicts_oldest_beyond_cap() {
        let mut seen = SeenListings::new(2);
        seen.observe("a");
        seen.observe("b");
        seen.observe("c"); // evicts "a"
        assert!(seen.observe("a")); // "a" was forgotten, looks new again
        assert!(!seen.observe("c"));
    }
}
