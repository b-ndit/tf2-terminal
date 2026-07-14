use serde::Deserialize;

use crate::error::AppResult;
use crate::infra::steam::SteamApiClient;

const GET_TRADE_OFFERS_URL: &str = "https://api.steampowered.com/IEconService/GetTradeOffers/v1/";

/// `ETradeOfferState::k_ETradeOfferStateActive` — the only state Module 9
/// analyzes. Other states (needs confirmation, in escrow, countered, ...)
/// aren't something a passive "analysis only" tool (`docs/DESIGN.md` §2)
/// can usefully act on yet.
const TRADE_OFFER_STATE_ACTIVE: i32 = 2;

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
}

impl TradeOffer {
    pub fn is_active(&self) -> bool {
        self.trade_offer_state == TRADE_OFFER_STATE_ACTIVE
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
                        "time_created": 1700000000
                    },
                    {
                        "tradeofferid": 6234567890123456790,
                        "accountid_other": 987654321,
                        "trade_offer_state": 6,
                        "time_created": 1700000001
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
                        "time_created": 3
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
    }
}
