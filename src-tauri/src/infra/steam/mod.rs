pub mod auth;
pub mod inventory;
pub mod schema;
pub mod trade_offers;

use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{DefaultDirectRateLimiter, Quota};
use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};

/// Longest response-body prefix logged on a decode failure — enough to see
/// the actual shape Steam sent without dumping an unbounded body into the
/// log file.
const LOGGED_BODY_PREFIX: usize = 2000;

/// Rate-limited HTTP client for the Steam Web API. One instance shared
/// across all Steam infra callers (schema sync now, inventory/trade offers
/// in Module 3) so the quota is global per host, per `docs/DESIGN.md` §11.
pub struct SteamApiClient {
    http: reqwest::Client,
    limiter: Arc<DefaultDirectRateLimiter>,
}

impl SteamApiClient {
    pub fn new() -> Self {
        // Steam's Web API allows far more than this; 4 req/s keeps us well
        // clear of any per-key throttling while still making a full schema
        // sync (dozens of paginated calls) finish in a few seconds.
        let quota = Quota::per_second(NonZeroU32::new(4).expect("4 is nonzero"));
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("tf2-terminal/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build reqwest client"),
            limiter: Arc::new(governor::RateLimiter::direct(quota)),
        }
    }

    /// Fetches `text()` rather than using `reqwest`'s own `.json()` so a
    /// decode failure can log the actual response body — `reqwest`'s error
    /// alone gives no way to see what Steam sent that didn't match our
    /// struct (verified live: this is what turned an opaque "error
    /// decoding response body" report into a fixable bug).
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> AppResult<T> {
        self.limiter.until_ready().await;
        let response = self.http.get(url).query(query).send().await?;
        let response = response.error_for_status()?;
        let body = response.text().await?;
        serde_json::from_str(&body).map_err(|e| {
            let snippet: String = body.chars().take(LOGGED_BODY_PREFIX).collect();
            tracing::error!(url, error = %e, body = %snippet, "failed to decode Steam API JSON response");
            AppError::Network(format!("failed to decode response from {url}: {e}"))
        })
    }
}

impl Default for SteamApiClient {
    fn default() -> Self {
        Self::new()
    }
}
