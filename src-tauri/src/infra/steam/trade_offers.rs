use serde::{Deserialize, Deserializer};

use crate::error::AppResult;
use crate::infra::steam::SteamApiClient;

/// Steam's Web API is inconsistent about whether large integer fields come
/// back as JSON numbers or JSON strings (a widely-documented quirk —
/// `tradeofferid` values are large enough to raise it) — serde doesn't
/// coerce between the two by default, so a plain `u64` field panics with
/// a "decoding response body" error the moment Steam happens to quote it.
/// Accept either representation.
fn u64_from_number_or_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString {
        Number(u64),
        String(String),
    }
    match NumberOrString::deserialize(deserializer)? {
        NumberOrString::Number(n) => Ok(n),
        NumberOrString::String(s) => s.parse().map_err(serde::de::Error::custom),
    }
}

const GET_TRADE_OFFERS_URL: &str = "https://api.steampowered.com/IEconService/GetTradeOffers/v1/";

/// `ETradeOfferState::k_ETradeOfferStateActive` — the only state Module 9
/// analyzes. Other states (needs confirmation, in escrow, countered, ...)
/// aren't something a passive "analysis only" tool (`docs/DESIGN.md` §2)
/// can usefully act on yet.
const TRADE_OFFER_STATE_ACTIVE: i32 = 2;
/// `ETradeOfferState::k_ETradeOfferStateAccepted` — a completed trade,
/// Module 12's `TradeHistoryService` imports these into the `trades`
/// ledger.
const TRADE_OFFER_STATE_ACCEPTED: i32 = 3;

/// One item reference within a trade offer. Steam quotes `assetid` as a
/// JSON string (it can exceed a JS-safe integer), which conveniently
/// matches it directly against `TF2Item::id.to_string()` from
/// `infra::steam::inventory` with no numeric parsing needed.
#[derive(Debug, Clone, Deserialize)]
pub struct TradeOfferAsset {
    pub assetid: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradeOffer {
    #[serde(deserialize_with = "u64_from_number_or_string")]
    pub tradeofferid: u64,
    pub accountid_other: u32,
    #[serde(default)]
    pub message: String,
    pub trade_offer_state: i32,
    #[serde(default)]
    pub items_to_give: Vec<TradeOfferAsset>,
    #[serde(default)]
    pub items_to_receive: Vec<TradeOfferAsset>,
    pub time_created: i64,
    /// Last state-change time — Steam bumps this on every transition, so
    /// for an Accepted offer this is effectively "when the trade
    /// completed" (Module 12's `trades.completed_ts`).
    pub time_updated: i64,
}

impl TradeOffer {
    pub fn is_active(&self) -> bool {
        self.trade_offer_state == TRADE_OFFER_STATE_ACTIVE
    }

    pub fn is_accepted(&self) -> bool {
        self.trade_offer_state == TRADE_OFFER_STATE_ACCEPTED
    }
}

#[derive(Debug, Deserialize)]
struct Envelope {
    response: TradeOffersResult,
}

#[derive(Debug, Deserialize)]
struct TradeOffersResult {
    #[serde(default)]
    trade_offers_received: Vec<TradeOffer>,
    #[serde(default)]
    trade_offers_sent: Vec<TradeOffer>,
}

pub struct SteamTradeOfferClient<'a> {
    api: &'a SteamApiClient,
    api_key: String,
}

impl<'a> SteamTradeOfferClient<'a> {
    pub fn new(api: &'a SteamApiClient, api_key: String) -> Self {
        Self { api, api_key }
    }

    /// Active offers the user has received, awaiting their decision.
    /// `is_our_offer`/sent offers are out of scope — v1 only analyzes
    /// incoming trades (`docs/DESIGN.md` §9's "INCOMING TRADE" panel).
    pub async fn fetch_active_received_offers(&self) -> AppResult<Vec<TradeOffer>> {
        let envelope: Envelope = self
            .api
            .get_json(
                GET_TRADE_OFFERS_URL,
                &[
                    ("key", self.api_key.as_str()),
                    ("get_received_offers", "1"),
                    ("get_sent_offers", "0"),
                    ("active_only", "1"),
                ],
            )
            .await?;

        Ok(envelope
            .response
            .trade_offers_received
            .into_iter()
            .filter(TradeOffer::is_active)
            .collect())
    }

    /// Completed trades (either sent or received — Module 12's ledger
    /// doesn't care which side initiated) updated at or after `since_ts`.
    /// `historical_only=1` requires `time_historical_cutoff`; `active_only`
    /// is deliberately *not* set (its default already includes historical
    /// states when paired with `historical_only`).
    pub async fn fetch_completed_offers(&self, since_ts: i64) -> AppResult<Vec<TradeOffer>> {
        let cutoff = since_ts.to_string();
        let envelope: Envelope = self
            .api
            .get_json(
                GET_TRADE_OFFERS_URL,
                &[
                    ("key", self.api_key.as_str()),
                    ("get_received_offers", "1"),
                    ("get_sent_offers", "1"),
                    ("historical_only", "1"),
                    ("time_historical_cutoff", cutoff.as_str()),
                ],
            )
            .await?;

        Ok(envelope
            .response
            .trade_offers_received
            .into_iter()
            .chain(envelope.response.trade_offers_sent)
            .filter(TradeOffer::is_accepted)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tradeofferid_accepts_either_a_json_number_or_a_quoted_string() {
        let numeric = r#"{"tradeofferid": 6234567890123456789, "accountid_other": 1, "trade_offer_state": 2, "time_created": 1, "time_updated": 1}"#;
        let stringly = r#"{"tradeofferid": "6234567890123456789", "accountid_other": 1, "trade_offer_state": 2, "time_created": 1, "time_updated": 1}"#;

        let from_number: TradeOffer = serde_json::from_str(numeric).unwrap();
        let from_string: TradeOffer = serde_json::from_str(stringly).unwrap();

        assert_eq!(from_number.tradeofferid, 6234567890123456789);
        assert_eq!(from_string.tradeofferid, 6234567890123456789);
    }

    #[test]
    fn parses_envelope_with_active_and_inactive_offers() {
        let json = r#"{
            "response": {
                "trade_offers_received": [
                    {
                        "tradeofferid": 6234567890123456789,
                        "accountid_other": 123456789,
                        "message": "trade me",
                        "trade_offer_state": 2,
                        "items_to_give": [{"assetid": "111"}],
                        "items_to_receive": [{"assetid": "222"}, {"assetid": "333"}],
                        "time_created": 1700000000,
                        "time_updated": 1700000000
                    },
                    {
                        "tradeofferid": 6234567890123456790,
                        "accountid_other": 987654321,
                        "trade_offer_state": 6,
                        "time_created": 1700000001,
                        "time_updated": 1700000002
                    }
                ]
            }
        }"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.response.trade_offers_received.len(), 2);

        let active = &envelope.response.trade_offers_received[0];
        assert!(active.is_active());
        assert_eq!(active.items_to_give.len(), 1);
        assert_eq!(active.items_to_receive.len(), 2);
        assert_eq!(active.message, "trade me");

        let cancelled = &envelope.response.trade_offers_received[1];
        assert!(!cancelled.is_active());
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let json = r#"{
            "response": {
                "trade_offers_received": [
                    {
                        "tradeofferid": 1,
                        "accountid_other": 2,
                        "trade_offer_state": 2,
                        "time_created": 3,
                        "time_updated": 4
                    }
                ]
            }
        }"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();
        let offer = &envelope.response.trade_offers_received[0];
        assert_eq!(offer.message, "");
        assert!(offer.items_to_give.is_empty());
        assert!(offer.items_to_receive.is_empty());
    }

    #[test]
    fn empty_response_parses_to_empty_vec() {
        let json = r#"{"response": {}}"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert!(envelope.response.trade_offers_received.is_empty());
        assert!(envelope.response.trade_offers_sent.is_empty());
    }

    #[test]
    fn is_accepted_is_true_only_for_state_3() {
        let json = r#"{
            "response": {
                "trade_offers_received": [
                    {"tradeofferid": 1, "accountid_other": 2, "trade_offer_state": 3, "time_created": 1, "time_updated": 5},
                    {"tradeofferid": 2, "accountid_other": 2, "trade_offer_state": 2, "time_created": 1, "time_updated": 1}
                ]
            }
        }"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert!(envelope.response.trade_offers_received[0].is_accepted());
        assert!(!envelope.response.trade_offers_received[1].is_accepted());
    }

    #[test]
    fn parses_sent_offers_separately_from_received() {
        let json = r#"{
            "response": {
                "trade_offers_received": [
                    {"tradeofferid": 1, "accountid_other": 2, "trade_offer_state": 3, "time_created": 1, "time_updated": 5}
                ],
                "trade_offers_sent": [
                    {"tradeofferid": 2, "accountid_other": 3, "trade_offer_state": 3, "time_created": 2, "time_updated": 6}
                ]
            }
        }"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.response.trade_offers_received.len(), 1);
        assert_eq!(envelope.response.trade_offers_sent.len(), 1);
        assert_eq!(envelope.response.trade_offers_sent[0].tradeofferid, 2);
        assert_eq!(envelope.response.trade_offers_sent[0].time_updated, 6);
    }
}
