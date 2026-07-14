use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{AppError, AppResult};
use crate::infra::backpack_tf::models::{WsEnvelope, WsListingPayload};

const WS_URL: &str = "wss://ws.backpack.tf/events";
const MAX_BACKOFF_SECS: u64 = 60;

/// A parsed websocket frame, before `services::market_data_service` decides
/// whether an upsert is a genuinely new listing or an update to one it's
/// already seen. Verified live: `listing-delete` carries the same full
/// payload shape as `listing-update` (not just an id).
pub enum RawListingEvent {
    Upserted(WsListingPayload),
    Deleted(WsListingPayload),
}

/// Connects once and forwards parsed events until the connection drops or
/// errors. Malformed individual frames are logged and skipped rather than
/// killing the whole connection.
async fn connect_and_stream(tx: &mpsc::Sender<RawListingEvent>) -> AppResult<()> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(WS_URL)
        .await
        .map_err(|e| AppError::Network(format!("backpack.tf websocket connect failed: {e}")))?;
    tracing::info!("connected to backpack.tf websocket");

    let (_, mut read) = ws_stream.split();
    while let Some(message) = read.next().await {
        let message =
            message.map_err(|e| AppError::Network(format!("backpack.tf websocket error: {e}")))?;
        let Message::Text(text) = message else {
            continue;
        };

        let envelopes: Vec<WsEnvelope> = match serde_json::from_str(&text) {
            Ok(envelopes) => envelopes,
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse backpack.tf websocket frame, skipping");
                continue;
            }
        };

        for envelope in envelopes {
            let raw = match envelope.event.as_str() {
                "listing-update" => RawListingEvent::Upserted(envelope.payload),
                "listing-delete" => RawListingEvent::Deleted(envelope.payload),
                _ => continue,
            };
            if tx.send(raw).await.is_err() {
                // Receiver dropped — nothing more to do.
                return Ok(());
            }
        }
    }

    Ok(())
}

/// Runs `connect_and_stream` forever, reconnecting with exponential backoff
/// on disconnect/error. backpack.tf's own docs note the websocket server
/// "may restart occasionally" and to expect this.
pub async fn run_with_reconnect(tx: mpsc::Sender<RawListingEvent>) {
    let mut backoff = 1u64;
    loop {
        match connect_and_stream(&tx).await {
            Ok(()) => tracing::warn!("backpack.tf websocket closed, reconnecting"),
            Err(e) => tracing::warn!(error = %e, "backpack.tf websocket error, reconnecting"),
        }
        if tx.is_closed() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF_SECS);
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::backpack_tf::models::WsEnvelope;

    #[test]
    fn parses_real_listing_update_shape() {
        // Real shape captured live from wss://ws.backpack.tf/events.
        let json = r##"[{
            "id": "6a5574a345d21825ff076999",
            "event": "listing-update",
            "payload": {
                "id": "440_76561199072632479_0aa4357eb8d3ed827ae2921807c27332",
                "steamid": "76561199072632479",
                "intent": "buy",
                "value": {"raw": 1268.39, "short": "22.17 keys", "long": "22 keys, 9.55 ref"},
                "item": {
                    "appid": 440,
                    "defindex": 30976,
                    "quality": {"id": 5, "name": "Unusual", "color": "#8650AC"}
                }
            }
        }]"##;
        let envelopes: Vec<WsEnvelope> = serde_json::from_str(json).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].event, "listing-update");
        assert_eq!(envelopes[0].payload.item.defindex, 30976);
        assert_eq!(envelopes[0].payload.item.quality.id, 5);
        assert_eq!(envelopes[0].payload.value.as_ref().unwrap().raw, 1268.39);
    }

    #[test]
    fn parses_listing_delete_shape() {
        let json = r##"[{
            "id": "abc",
            "event": "listing-delete",
            "payload": {
                "id": "440_76561199072632479_deadbeef",
                "steamid": "76561199072632479",
                "intent": "sell",
                "item": {"appid": 440, "defindex": 1, "quality": {"id": 6, "name": "Unique", "color": "#7D6D00"}}
            }
        }]"##;
        let envelopes: Vec<WsEnvelope> = serde_json::from_str(json).unwrap();
        assert_eq!(envelopes[0].event, "listing-delete");
        assert_eq!(envelopes[0].payload.id, "440_76561199072632479_deadbeef");
    }

    #[test]
    fn parses_particle_and_user_name_when_present() {
        // Real shape captured live: an Unusual listing includes the
        // particle effect id and the lister's display name.
        let json = r##"[{
            "id": "6a5574a345d21825ff076999",
            "event": "listing-update",
            "payload": {
                "id": "440_76561199072632479_0aa4357eb8d3ed827ae2921807c27332",
                "steamid": "76561199072632479",
                "intent": "buy",
                "value": {"raw": 1268.39, "short": "22.17 keys", "long": "22 keys, 9.55 ref"},
                "item": {
                    "appid": 440,
                    "defindex": 30976,
                    "quality": {"id": 5, "name": "Unusual", "color": "#8650AC"},
                    "particle": {"id": 701, "name": "Hot"}
                },
                "user": {"id": "76561199072632479", "name": "5ScrapyardBot"}
            }
        }]"##;
        let envelopes: Vec<WsEnvelope> = serde_json::from_str(json).unwrap();
        assert_eq!(envelopes[0].payload.item.particle.as_ref().unwrap().id, 701);
        assert_eq!(
            envelopes[0].payload.user.as_ref().unwrap().name,
            "5ScrapyardBot"
        );
    }

    #[test]
    fn particle_and_user_are_optional() {
        let json = r##"[{
            "id": "abc",
            "event": "listing-update",
            "payload": {
                "id": "x", "steamid": "y", "intent": "sell",
                "item": {"appid": 440, "defindex": 1, "quality": {"id": 6, "name": "Unique", "color": "#7D6D00"}}
            }
        }]"##;
        let envelopes: Vec<WsEnvelope> = serde_json::from_str(json).unwrap();
        assert!(envelopes[0].payload.item.particle.is_none());
        assert!(envelopes[0].payload.user.is_none());
    }

    #[test]
    fn ignores_unknown_event_types_gracefully() {
        let json = r##"[{
            "id": "abc",
            "event": "some-future-event-type",
            "payload": {
                "id": "x", "steamid": "y", "intent": "sell",
                "item": {"appid": 440, "defindex": 1, "quality": {"id": 6, "name": "Unique", "color": "#7D6D00"}}
            }
        }]"##;
        let envelopes: Vec<WsEnvelope> = serde_json::from_str(json).unwrap();
        assert_eq!(envelopes[0].event, "some-future-event-type");
    }
}
