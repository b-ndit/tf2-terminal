use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::infra::steam::SteamApiClient;

const SCHEMA_OVERVIEW_URL: &str =
    "https://api.steampowered.com/IEconItems_440/GetSchemaOverview/v0001/";
const SCHEMA_ITEMS_URL: &str = "https://api.steampowered.com/IEconItems_440/GetSchemaItems/v0001/";

/// One base item definition from Valve's schema (a weapon, hat slot, etc. —
/// not a specific SKU permutation; that's [`crate::domain::item::ItemKey`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaItem {
    pub defindex: u32,
    pub item_name: String,
    #[serde(default)]
    pub item_quality: u8,
    #[serde(default)]
    pub item_class: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
}

/// An Unusual particle effect (`attribute_controlled_attached_particles` in
/// `GetSchemaOverview`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleEffect {
    pub id: u32,
    pub name: String,
}

/// Parsed, dynamic parts of the schema overview. Quality *ids* are stable
/// and hardcoded in [`crate::domain::item::Quality`]; what's actually worth
/// caching here is the effect catalog, which grows with every update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaOverview {
    pub quality_names: HashMap<u8, String>,
    pub particles: HashMap<u32, String>,
}

#[derive(Debug, Deserialize)]
struct OverviewEnvelope {
    result: OverviewResult,
}

#[derive(Debug, Deserialize)]
struct OverviewResult {
    qualities: HashMap<String, u8>,
    #[serde(rename = "qualityNames")]
    quality_names: HashMap<String, String>,
    #[serde(default)]
    attribute_controlled_attached_particles: Vec<ParticleEffect>,
}

#[derive(Debug, Deserialize)]
struct ItemsEnvelope {
    result: ItemsResult,
}

#[derive(Debug, Deserialize)]
struct ItemsResult {
    items: Vec<SchemaItem>,
    next: Option<u32>,
}

pub struct SteamSchemaClient<'a> {
    api: &'a SteamApiClient,
    api_key: String,
}

impl<'a> SteamSchemaClient<'a> {
    pub fn new(api: &'a SteamApiClient, api_key: String) -> Self {
        Self { api, api_key }
    }

    pub async fn fetch_overview(&self) -> AppResult<SchemaOverview> {
        let envelope: OverviewEnvelope = self
            .api
            .get_json(
                SCHEMA_OVERVIEW_URL,
                &[("key", self.api_key.as_str()), ("language", "en")],
            )
            .await?;

        let quality_names = envelope
            .result
            .qualities
            .iter()
            .filter_map(|(internal_name, &id)| {
                envelope
                    .result
                    .quality_names
                    .get(internal_name)
                    .map(|display_name| (id, display_name.clone()))
            })
            .collect();

        let particles = envelope
            .result
            .attribute_controlled_attached_particles
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect();

        Ok(SchemaOverview {
            quality_names,
            particles,
        })
    }

    /// Depaginates the full item catalog (~1000 items per page as of the
    /// live API).
    pub async fn fetch_all_items(&self) -> AppResult<Vec<SchemaItem>> {
        let mut all_items = Vec::new();
        let mut start = Some(0u32);

        while let Some(cursor) = start {
            let cursor_str = cursor.to_string();
            let envelope: ItemsEnvelope = self
                .api
                .get_json(
                    SCHEMA_ITEMS_URL,
                    &[
                        ("key", self.api_key.as_str()),
                        ("language", "en"),
                        ("start", cursor_str.as_str()),
                    ],
                )
                .await
                .map_err(|e| AppError::Network(format!("schema items page at {cursor}: {e}")))?;

            all_items.extend(envelope.result.items);
            start = envelope.result.next;
        }

        Ok(all_items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_real_schema_item_shape() {
        let json = r#"{
            "name": "TF_WEAPON_BAT",
            "defindex": 0,
            "item_class": "tf_weapon_bat",
            "item_name": "Bat",
            "item_quality": 0,
            "image_url": "http://example.com/bat.png"
        }"#;
        let item: SchemaItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.defindex, 0);
        assert_eq!(item.item_name, "Bat");
        assert_eq!(item.item_quality, 0);
        assert_eq!(item.item_class.as_deref(), Some("tf_weapon_bat"));
    }

    #[test]
    fn deserializes_schema_item_missing_optional_fields() {
        let json = r#"{"defindex": 5021, "item_name": "Mann Co. Supply Crate Key"}"#;
        let item: SchemaItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.item_quality, 0);
        assert_eq!(item.item_class, None);
        assert_eq!(item.image_url, None);
    }

    #[test]
    fn parses_items_page_with_pagination_cursor() {
        let json = r#"{
            "result": {
                "status": 1,
                "items": [{"defindex": 1, "item_name": "Bottle", "item_quality": 0}],
                "next": 1150
            }
        }"#;
        let envelope: ItemsEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.result.items.len(), 1);
        assert_eq!(envelope.result.next, Some(1150));
    }

    #[test]
    fn parses_final_items_page_without_cursor() {
        let json = r#"{
            "result": {
                "status": 1,
                "items": []
            }
        }"#;
        let envelope: ItemsEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.result.next, None);
    }

    #[test]
    fn overview_cross_references_qualities_and_quality_names_by_internal_key() {
        let json = r#"{
            "result": {
                "status": 1,
                "qualities": {"Normal": 0, "rarity1": 1, "Unique": 6},
                "qualityNames": {"Normal": "Normal", "rarity1": "Genuine", "Unique": "Unique"},
                "attribute_controlled_attached_particles": [
                    {"id": 4, "name": "Community Sparkle"}
                ]
            }
        }"#;
        let envelope: OverviewEnvelope = serde_json::from_str(json).unwrap();

        let quality_names: HashMap<u8, String> = envelope
            .result
            .qualities
            .iter()
            .filter_map(|(k, &id)| {
                envelope
                    .result
                    .quality_names
                    .get(k)
                    .map(|n| (id, n.clone()))
            })
            .collect();

        assert_eq!(quality_names.get(&1), Some(&"Genuine".to_string()));
        assert_eq!(quality_names.get(&6), Some(&"Unique".to_string()));
        assert_eq!(
            envelope.result.attribute_controlled_attached_particles[0].name,
            "Community Sparkle"
        );
    }
}
