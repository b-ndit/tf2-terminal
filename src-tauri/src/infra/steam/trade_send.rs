//! Sends/accepts/declines real Steam trade offers via
//! `steamcommunity.com`'s unofficial, session-cookie-authenticated
//! endpoints — distinct from `infra::steam::trade_offers`'s
//! `IEconService` client, which only reads offers using the Web API key.
//! Steam has no official "create a trade offer" or "accept a trade offer"
//! Web API method; both require a real logged-in browser session
//! (`sessionid` + `steamLoginSecure` cookies), which the user pastes in
//! manually from their own browser (Settings) — this app otherwise never
//! holds Steam session credentials (`docs/DESIGN.md` §2).
//!
//! Deliberately a separate client (not folded into `SteamApiClient`,
//! which is GET-only, `api.steampowered.com`-only, and keyed by `?key=`
//! rather than cookies) with its own conservative rate limit — these are
//! unofficial endpoints, and aggressive calling risks a ban in a way the
//! official Web API doesn't.

use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{DefaultDirectRateLimiter, Quota};
use serde::{Deserialize, Serialize};

use crate::domain::steam_id::SteamId64;
use crate::error::{AppError, AppResult};

const TF2_APP_ID: u32 = 440;
const TF2_CONTEXT_ID: &str = "2";

fn new_offer_url() -> String {
    "https://steamcommunity.com/tradeoffer/new/send".to_string()
}

fn accept_url(trade_offer_id: u64) -> String {
    format!("https://steamcommunity.com/tradeoffer/{trade_offer_id}/accept")
}

fn decline_url(trade_offer_id: u64) -> String {
    format!("https://steamcommunity.com/tradeoffer/{trade_offer_id}/decline")
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct JsonTradeOfferAsset {
    appid: u32,
    contextid: String,
    assetid: String,
    amount: u32,
}

fn to_json_assets(asset_ids: &[String]) -> Vec<JsonTradeOfferAsset> {
    asset_ids
        .iter()
        .map(|assetid| JsonTradeOfferAsset {
            appid: TF2_APP_ID,
            contextid: TF2_CONTEXT_ID.to_string(),
            assetid: assetid.clone(),
            amount: 1,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct JsonTradeOfferSide {
    assets: Vec<JsonTradeOfferAsset>,
    currency: Vec<serde_json::Value>,
    ready: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct JsonTradeOffer {
    newversion: bool,
    version: u32,
    me: JsonTradeOfferSide,
    them: JsonTradeOfferSide,
}

/// Builds the `json_tradeoffer` POST field's value — pure, so it's
/// unit-testable without a live Steam session. `my_assets`/`their_assets`
/// are Steam asset ids (TF2's `contextid` is always `"2"`; there's only
/// ever one inventory context per app).
fn build_json_tradeoffer(my_assets: &[String], their_assets: &[String]) -> String {
    let offer = JsonTradeOffer {
        newversion: true,
        version: 4,
        me: JsonTradeOfferSide {
            assets: to_json_assets(my_assets),
            currency: Vec::new(),
            ready: false,
        },
        them: JsonTradeOfferSide {
            assets: to_json_assets(their_assets),
            currency: Vec::new(),
            ready: false,
        },
    };
    serde_json::to_string(&offer).expect("JsonTradeOffer always serializes")
}

fn cookie_header(session_id: &str, login_secure: &str) -> String {
    format!("sessionid={session_id}; steamLoginSecure={login_secure}")
}

#[derive(Debug, Deserialize)]
struct SendOfferResponse {
    #[serde(default)]
    tradeofferid: Option<String>,
    #[serde(default, rename = "strError")]
    str_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GenericSteamResponse {
    #[serde(default, rename = "strError")]
    str_error: Option<String>,
}

/// Rate-limited HTTP client for Steam's unofficial, session-cookie
/// authenticated trade-offer endpoints (send/accept/decline) — see the
/// module doc comment for why this is a separate client from
/// `SteamApiClient`/`trade_offers::SteamTradeOfferClient`.
pub struct SteamSessionClient {
    http: reqwest::Client,
    limiter: Arc<DefaultDirectRateLimiter>,
}

impl SteamSessionClient {
    pub fn new() -> Self {
        // Conservative on purpose — unofficial endpoints, real account risk.
        let quota = Quota::per_second(NonZeroU32::new(1).expect("1 is nonzero"));
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("tf2-terminal/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build reqwest client"),
            limiter: Arc::new(governor::RateLimiter::direct(quota)),
        }
    }

    /// Sends a new trade offer to `partner`. Returns the new offer's id.
    /// The `Referer` header uses `partner.account_id()` (32-bit) — the form
    /// itself as it appears in a real browser is keyed the same way.
    pub async fn send_offer(
        &self,
        session_id: &str,
        login_secure: &str,
        partner: SteamId64,
        my_assets: &[String],
        their_assets: &[String],
        message: &str,
    ) -> AppResult<u64> {
        self.limiter.until_ready().await;

        let json_tradeoffer = build_json_tradeoffer(my_assets, their_assets);
        let partner_str = partner.to_string();
        let referer = format!(
            "https://steamcommunity.com/tradeoffer/new/?partner={}",
            partner.account_id()
        );

        let response = self
            .http
            .post(new_offer_url())
            .header(reqwest::header::COOKIE, cookie_header(session_id, login_secure))
            .header(reqwest::header::REFERER, referer)
            .form(&[
                ("sessionid", session_id),
                ("serverid", "1"),
                ("partner", partner_str.as_str()),
                ("tradeoffermessage", message),
                ("json_tradeoffer", json_tradeoffer.as_str()),
                ("trade_offer_create_params", "{}"),
            ])
            .send()
            .await?
            .error_for_status()?;

        let body = response.text().await?;
        let parsed: SendOfferResponse = serde_json::from_str(&body).map_err(|e| {
            tracing::error!(error = %e, body = %body, "failed to decode Steam trade-offer send response");
            AppError::Network(format!("failed to decode Steam's response: {e}"))
        })?;

        if let Some(err) = parsed.str_error {
            return Err(AppError::Network(format!(
                "Steam rejected the trade offer: {err}"
            )));
        }
        let tradeofferid = parsed
            .tradeofferid
            .ok_or_else(|| AppError::Network("Steam did not return a trade offer id".to_string()))?;
        tradeofferid
            .parse::<u64>()
            .map_err(|e| AppError::Network(format!("invalid trade offer id from Steam: {e}")))
    }

    /// Accepts an already-active offer the user received.
    pub async fn accept_offer(
        &self,
        session_id: &str,
        login_secure: &str,
        trade_offer_id: u64,
        partner: SteamId64,
    ) -> AppResult<()> {
        self.limiter.until_ready().await;

        let referer = format!("https://steamcommunity.com/tradeoffer/{trade_offer_id}/");
        let trade_offer_id_str = trade_offer_id.to_string();
        let partner_str = partner.to_string();
        let response = self
            .http
            .post(accept_url(trade_offer_id))
            .header(reqwest::header::COOKIE, cookie_header(session_id, login_secure))
            .header(reqwest::header::REFERER, referer)
            .form(&[
                ("sessionid", session_id),
                ("serverid", "1"),
                ("tradeofferid", trade_offer_id_str.as_str()),
                ("partner", partner_str.as_str()),
                ("captcha", ""),
            ])
            .send()
            .await?
            .error_for_status()?;

        self.check_for_steam_error(response).await
    }

    /// Declines an already-active offer the user received.
    pub async fn decline_offer(
        &self,
        session_id: &str,
        login_secure: &str,
        trade_offer_id: u64,
    ) -> AppResult<()> {
        self.limiter.until_ready().await;

        let referer = format!("https://steamcommunity.com/tradeoffer/{trade_offer_id}/");
        let response = self
            .http
            .post(decline_url(trade_offer_id))
            .header(reqwest::header::COOKIE, cookie_header(session_id, login_secure))
            .header(reqwest::header::REFERER, referer)
            .form(&[("sessionid", session_id)])
            .send()
            .await?
            .error_for_status()?;

        self.check_for_steam_error(response).await
    }

    /// Accept/decline both return a 200 with a JSON body even when Steam
    /// rejects the action (e.g. the offer already changed state) — a
    /// non-2xx alone isn't a reliable failure signal, so this also checks
    /// the body for `strError`.
    async fn check_for_steam_error(&self, response: reqwest::Response) -> AppResult<()> {
        let body = response.text().await?;
        if body.trim().is_empty() {
            return Ok(());
        }
        if let Ok(parsed) = serde_json::from_str::<GenericSteamResponse>(&body) {
            if let Some(err) = parsed.str_error {
                return Err(AppError::Network(format!("Steam rejected the request: {err}")));
            }
        }
        Ok(())
    }
}

impl Default for SteamSessionClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_json_tradeoffer_includes_both_sides_assets_with_tf2_app_context() {
        let json = build_json_tradeoffer(
            &["111".to_string(), "222".to_string()],
            &["333".to_string()],
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["newversion"], true);
        assert_eq!(parsed["version"], 4);
        assert_eq!(parsed["me"]["assets"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["them"]["assets"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["me"]["assets"][0]["appid"], 440);
        assert_eq!(parsed["me"]["assets"][0]["contextid"], "2");
        assert_eq!(parsed["me"]["assets"][0]["assetid"], "111");
        assert_eq!(parsed["me"]["assets"][0]["amount"], 1);
    }

    #[test]
    fn build_json_tradeoffer_handles_empty_sides() {
        let json = build_json_tradeoffer(&[], &[]);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["me"]["assets"].as_array().unwrap().is_empty());
        assert!(parsed["them"]["assets"].as_array().unwrap().is_empty());
    }

    #[test]
    fn cookie_header_includes_both_cookies() {
        let header = cookie_header("sid123", "secure456");
        assert_eq!(header, "sessionid=sid123; steamLoginSecure=secure456");
    }

    #[test]
    fn send_offer_response_parses_success() {
        let body = r#"{"tradeofferid": "6234567890123456789"}"#;
        let parsed: SendOfferResponse = serde_json::from_str(body).unwrap();
        assert_eq!(
            parsed.tradeofferid,
            Some("6234567890123456789".to_string())
        );
        assert_eq!(parsed.str_error, None);
    }

    #[test]
    fn send_offer_response_parses_steam_error() {
        let body = r#"{"success": false, "strError": "There was an error sending your trade offer."}"#;
        let parsed: SendOfferResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.tradeofferid, None);
        assert_eq!(
            parsed.str_error,
            Some("There was an error sending your trade offer.".to_string())
        );
    }
}
