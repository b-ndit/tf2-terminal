use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota};
use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};
use crate::infra::backpack_tf::models::{PriceCatalogEnvelope, PriceCatalogResponse};

const MAX_RETRIES: u32 = 3;
const GET_PRICES_URL: &str = "https://backpack.tf/api/IGetPrices/v4";

/// Rate-limited HTTP client for backpack.tf's REST API. Honors `Retry-After`
/// on 429s and backs off exponentially on 5xx, per `docs/DESIGN.md` §2's
/// compliance rules.
pub struct BackpackTfClient {
    http: reqwest::Client,
    limiter: Arc<DefaultDirectRateLimiter>,
}

impl BackpackTfClient {
    pub fn new() -> Self {
        // The price catalog is cached server-side for 900s and we don't
        // poll classifieds per-item anymore (websocket-first, see
        // docs/DESIGN.md §2), so a conservative quota is plenty.
        let quota = Quota::per_second(NonZeroU32::new(1).expect("1 is nonzero"));
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("tf2-terminal/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build reqwest client"),
            limiter: Arc::new(governor::RateLimiter::direct(quota)),
        }
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> AppResult<T> {
        for attempt in 0..=MAX_RETRIES {
            self.limiter.until_ready().await;
            let response = self.http.get(url).query(query).send().await?;

            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(2u64.pow(attempt + 1));
                tracing::warn!(
                    retry_after,
                    attempt,
                    "backpack.tf rate limited us, backing off"
                );
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if response.status().is_server_error() && attempt < MAX_RETRIES {
                let backoff = 2u64.pow(attempt + 1);
                tracing::warn!(backoff, attempt, status = %response.status(), "backpack.tf server error, retrying");
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                continue;
            }

            let response = response.error_for_status()?;
            return Ok(response.json::<T>().await?);
        }

        Err(AppError::Network(
            "backpack.tf request failed after retries".to_string(),
        ))
    }

    /// Fetches the community price catalog (suggested prices for the whole
    /// item catalog). The response itself is cached server-side for 900s.
    pub async fn fetch_price_catalog(&self, api_key: &str) -> AppResult<PriceCatalogResponse> {
        let envelope: PriceCatalogEnvelope = self
            .get_json(GET_PRICES_URL, &[("key", api_key), ("raw", "1")])
            .await?;
        if envelope.response.success != 1 {
            return Err(AppError::Network(
                "IGetPrices returned success=0".to_string(),
            ));
        }
        Ok(envelope.response)
    }
}

impl Default for BackpackTfClient {
    fn default() -> Self {
        Self::new()
    }
}
