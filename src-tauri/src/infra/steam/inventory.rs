use serde::{Deserialize, Serialize};

use crate::domain::item::{ItemError, ItemKey, KillstreakTier, Quality};
use crate::domain::steam_id::SteamId64;
use crate::error::{AppError, AppResult};
use crate::infra::steam::SteamApiClient;

const GET_PLAYER_ITEMS_URL: &str =
    "https://api.steampowered.com/IEconItems_440/GetPlayerItems/v0001/";

// Verified live against a real inventory (see docs/DESIGN.md §2) against
// GetSchemaOverview's `attributes` list, which also carries each
// attribute's `stored_as_integer` flag — that flag decides whether to read
// `.value` directly or `.float_value` (Steam pre-decodes float-typed
// attributes for us). These eight are the only ones Module 3 needs.
const ATTR_UNUSUAL_EFFECT: u32 = 134; // stored_as_integer: false -> float_value
const ATTR_TAUNT_UNUSUAL_EFFECT: u32 = 2041; // stored_as_integer: true -> value
const ATTR_KILLSTREAK_TIER: u32 = 2025; // stored_as_integer: false -> float_value
const ATTR_AUSTRALIUM: u32 = 2027; // stored_as_integer: true; presence implies true
const ATTR_FESTIVIZED: u32 = 2053; // stored_as_integer: false; presence implies true
const ATTR_STRANGE_COUNT: u32 = 214; // stored_as_integer: true -> value
const ATTR_CRAFT_NUMBER: u32 = 229; // stored_as_integer: true -> value
const ATTR_PAINT_RGB: u32 = 142; // stored_as_integer: false -> float_value

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TF2Attribute {
    pub defindex: u32,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub float_value: Option<f64>,
}

impl TF2Attribute {
    fn as_integer(&self) -> Option<i64> {
        self.value
            .as_ref()
            .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|u| u as i64)))
    }
}

/// One item as returned by `IEconItems_440/GetPlayerItems`. This is the raw
/// wire shape — [`TF2Item::item_key`] derives the domain [`ItemKey`] from
/// it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TF2Item {
    pub id: u64,
    pub defindex: u32,
    #[serde(default)]
    pub level: u32,
    pub quality: u8,
    #[serde(default)]
    pub flag_cannot_trade: bool,
    #[serde(default)]
    pub flag_cannot_craft: bool,
    #[serde(default)]
    pub attributes: Vec<TF2Attribute>,
}

impl TF2Item {
    fn attribute(&self, defindex: u32) -> Option<&TF2Attribute> {
        self.attributes.iter().find(|a| a.defindex == defindex)
    }

    pub fn effect_id(&self) -> Option<u32> {
        if let Some(f) = self
            .attribute(ATTR_UNUSUAL_EFFECT)
            .and_then(|a| a.float_value)
        {
            return Some(f.round() as u32);
        }
        self.attribute(ATTR_TAUNT_UNUSUAL_EFFECT)
            .and_then(TF2Attribute::as_integer)
            .map(|i| i as u32)
    }

    pub fn killstreak_tier(&self) -> KillstreakTier {
        self.attribute(ATTR_KILLSTREAK_TIER)
            .and_then(|a| a.float_value)
            .and_then(|f| KillstreakTier::try_from(f.round() as u8).ok())
            .unwrap_or_default()
    }

    pub fn is_australium(&self) -> bool {
        self.attribute(ATTR_AUSTRALIUM).is_some()
    }

    pub fn is_festivized(&self) -> bool {
        self.attribute(ATTR_FESTIVIZED).is_some()
    }

    pub fn strange_count(&self) -> Option<i64> {
        self.attribute(ATTR_STRANGE_COUNT)
            .and_then(TF2Attribute::as_integer)
    }

    pub fn craft_number(&self) -> Option<i64> {
        self.attribute(ATTR_CRAFT_NUMBER)
            .and_then(TF2Attribute::as_integer)
    }

    pub fn paint_rgb(&self) -> Option<i64> {
        self.attribute(ATTR_PAINT_RGB)
            .and_then(|a| a.float_value)
            .map(|f| f.round() as i64)
    }

    pub fn item_key(&self) -> Result<ItemKey, ItemError> {
        Ok(ItemKey {
            defindex: self.defindex,
            quality: Quality::try_from(self.quality)?,
            effect_id: self.effect_id(),
            killstreak_tier: self.killstreak_tier(),
            australium: self.is_australium(),
            festivized: self.is_festivized(),
            craftable: !self.flag_cannot_craft,
        })
    }
}

#[derive(Debug, Deserialize)]
struct PlayerItemsEnvelope {
    result: PlayerItemsResult,
}

#[derive(Debug, Deserialize)]
struct PlayerItemsResult {
    status: i32,
    #[serde(default)]
    items: Vec<TF2Item>,
}

pub struct SteamInventoryClient<'a> {
    api: &'a SteamApiClient,
    api_key: String,
}

impl<'a> SteamInventoryClient<'a> {
    pub fn new(api: &'a SteamApiClient, api_key: String) -> Self {
        Self { api, api_key }
    }

    pub async fn fetch_items(&self, steam_id: SteamId64) -> AppResult<Vec<TF2Item>> {
        let steam_id_str = steam_id.to_string();
        let envelope: PlayerItemsEnvelope = self
            .api
            .get_json(
                GET_PLAYER_ITEMS_URL,
                &[
                    ("key", self.api_key.as_str()),
                    ("steamid", steam_id_str.as_str()),
                    ("format", "json"),
                ],
            )
            .await?;

        if envelope.result.status != 1 {
            return Err(AppError::Network(format!(
                "GetPlayerItems returned status {} (inventory may be private)",
                envelope.result.status
            )));
        }

        Ok(envelope.result.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_from_json(json: &str) -> TF2Item {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn plain_unique_weapon_has_no_modifiers() {
        let item = item_from_json(r#"{"id": 1, "defindex": 45, "quality": 6}"#);
        let key = item.item_key().unwrap();
        assert_eq!(key.quality, Quality::Unique);
        assert_eq!(key.effect_id, None);
        assert_eq!(key.killstreak_tier, KillstreakTier::None);
        assert!(!key.australium);
        assert!(!key.festivized);
        assert!(key.craftable);
    }

    #[test]
    fn decodes_unusual_killstreak_decorated_weapon() {
        // Real shape verified live: an Unusual Specialized Killstreak
        // Decorated Weapon (defindex 200, quality 15/DecoratedWeapon).
        let item = item_from_json(
            r#"{
                "id": 1, "defindex": 200, "quality": 15,
                "attributes": [
                    {"defindex": 134, "value": 1143947264, "float_value": 701},
                    {"defindex": 2014, "value": 1065353216, "float_value": 1},
                    {"defindex": 2025, "value": 1073741824, "float_value": 2}
                ]
            }"#,
        );
        let key = item.item_key().unwrap();
        assert_eq!(key.quality, Quality::DecoratedWeapon);
        assert_eq!(key.effect_id, Some(701));
        assert_eq!(key.killstreak_tier, KillstreakTier::Specialized);
    }

    #[test]
    fn decodes_unusual_taunt_via_taunt_particle_attribute() {
        // Real shape verified live: taunts store the effect id as a plain
        // integer in `value` on attribute 2041, not float_value on 134.
        let item = item_from_json(
            r#"{
                "id": 1, "defindex": 1172, "quality": 5,
                "attributes": [
                    {"defindex": 2041, "value": 3206}
                ]
            }"#,
        );
        assert_eq!(item.effect_id(), Some(3206));
    }

    #[test]
    fn decodes_strange_count_and_craft_number_as_plain_integers() {
        let item = item_from_json(
            r#"{
                "id": 1, "defindex": 655, "quality": 11,
                "attributes": [
                    {"defindex": 214, "value": 4},
                    {"defindex": 229, "value": 184420}
                ]
            }"#,
        );
        assert_eq!(item.strange_count(), Some(4));
        assert_eq!(item.craft_number(), Some(184420));
    }

    #[test]
    fn decodes_paint_via_float_value() {
        let item = item_from_json(
            r#"{
                "id": 1, "defindex": 261, "quality": 6,
                "attributes": [
                    {"defindex": 142, "value": 1258093820, "float_value": 8289918}
                ]
            }"#,
        );
        assert_eq!(item.paint_rgb(), Some(8289918));
    }

    #[test]
    fn australium_and_festivized_are_presence_flags() {
        let item = item_from_json(
            r#"{
                "id": 1, "defindex": 39, "quality": 6,
                "attributes": [
                    {"defindex": 2027, "value": 1},
                    {"defindex": 2053, "value": 1065353216, "float_value": 1}
                ]
            }"#,
        );
        assert!(item.is_australium());
        assert!(item.is_festivized());
    }

    #[test]
    fn flag_cannot_craft_maps_to_craftable_false() {
        let item =
            item_from_json(r#"{"id": 1, "defindex": 45, "quality": 6, "flag_cannot_craft": true}"#);
        assert!(!item.item_key().unwrap().craftable);
    }

    #[test]
    fn attribute_with_non_numeric_value_does_not_break_parsing() {
        // Real shape: some attributes carry a string payload (e.g. custom
        // taunt names), unrelated to anything we decode.
        let item = item_from_json(
            r#"{
                "id": 1, "defindex": 1172, "quality": 5,
                "attributes": [
                    {"defindex": 606, "value": "Taunt.BumperCarGoLoop"}
                ]
            }"#,
        );
        assert!(item.item_key().is_ok());
    }

    #[test]
    fn rejects_unknown_quality() {
        let item = item_from_json(r#"{"id": 1, "defindex": 45, "quality": 99}"#);
        assert!(item.item_key().is_err());
    }

    #[test]
    fn parses_envelope_with_items() {
        let json = r#"{
            "result": {
                "status": 1,
                "num_backpack_slots": 2800,
                "items": [{"id": 1, "defindex": 45, "quality": 6}]
            }
        }"#;
        let envelope: PlayerItemsEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.result.status, 1);
        assert_eq!(envelope.result.items.len(), 1);
    }
}
