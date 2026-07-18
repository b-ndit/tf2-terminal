use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::domain::item::{ItemError, ItemKey, KillstreakTier, Quality};

/// One price observation. Verified live against the real `IGetPrices/v4`
/// response — several fields are nullable in practice (e.g. a
/// non-tradable/non-craftable combo with no trade history at all).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceEntry {
    pub value: Option<f64>,
    pub currency: Option<String>,
    #[serde(default)]
    pub difference: Option<f64>,
    #[serde(default)]
    pub last_update: Option<i64>,
    #[serde(default)]
    pub value_high: Option<f64>,
    #[serde(default)]
    pub value_raw: Option<f64>,
}

/// The `Craftable`/`Non-Craftable` value under a quality's `Tradable`
/// group is polymorphic depending on quality, verified live: plain items
/// give a `Vec<PriceEntry>` (usually one entry); Unusual (quality 5) gives
/// a map keyed by particle effect id instead. `#[serde(untagged)]` picks
/// whichever shape matches — a JSON array can never also parse as a map,
/// so this is unambiguous.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CraftableEntry {
    Plain(Vec<PriceEntry>),
    ByEffect(HashMap<String, PriceEntry>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradableGroup {
    #[serde(rename = "Craftable")]
    pub craftable: Option<CraftableEntry>,
    #[serde(rename = "Non-Craftable")]
    pub non_craftable: Option<CraftableEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityPrices {
    #[serde(rename = "Tradable")]
    pub tradable: Option<TradableGroup>,
    #[serde(rename = "Non-Tradable")]
    pub non_tradable: Option<TradableGroup>,
}

/// One named catalog entry. `defindex` is a list because several
/// defindexes (class-specific reskins etc.) can share one name/price —
/// and, verified live, can include synthetic sentinel values like `-2`
/// ("Random Craft Hat") that don't correspond to a real item, hence `i64`
/// rather than our validated domain `u32`. `prices` is keyed by quality id
/// as a string (matches the wire format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceCatalogItem {
    #[serde(default)]
    pub defindex: Vec<i64>,
    #[serde(default)]
    pub prices: HashMap<String, QualityPrices>,
}

#[derive(Debug, Deserialize)]
pub struct PriceCatalogEnvelope {
    pub response: PriceCatalogResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceCatalogResponse {
    pub success: i32,
    pub current_time: i64,
    #[serde(default)]
    pub items: HashMap<String, PriceCatalogItem>,
}

/// A single listing update/removal from the live websocket feed. Kind is
/// determined by `services::market_data_service` tracking which listing
/// ids it's already seen — the wire protocol only distinguishes
/// update-or-create (`listing-update`) from `listing-delete`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ListingEventKind {
    New,
    Updated,
    Removed,
}

/// Also `Deserialize` (added for Module 14) so a `market_provider` plugin's
/// `provide_listings` JSON output can be parsed directly into this type —
/// the same shape every other market data source already feeds into
/// `MarketDataService`'s broadcast bus.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ListingEvent {
    pub listing_id: String,
    pub kind: ListingEventKind,
    pub defindex: u32,
    pub quality: u8,
    pub effect_id: Option<u32>,
    pub killstreak_tier: u8,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
    pub intent: String,
    pub steam_id: String,
    pub steam_name: Option<String>,
    /// Total listing value already normalized to ref by backpack.tf
    /// (`payload.value.raw`).
    pub value_ref: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsEnvelope {
    pub event: String,
    pub payload: WsListingPayload,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsListingPayload {
    pub id: String,
    pub steamid: String,
    pub intent: String,
    #[serde(default)]
    pub value: Option<WsValue>,
    pub item: WsItem,
    #[serde(default)]
    pub user: Option<WsUser>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsValue {
    pub raw: f64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsItem {
    pub defindex: u32,
    pub quality: WsQuality,
    #[serde(default)]
    pub particle: Option<WsParticle>,
    // Verified live: present on real Strange/Killstreak/Australium/
    // Festivized listings, absent otherwise (hence the defaults below).
    #[serde(default, rename = "killstreakTier")]
    pub killstreak_tier: Option<u8>,
    #[serde(default)]
    pub australium: Option<bool>,
    #[serde(default)]
    pub festivized: Option<bool>,
    #[serde(default)]
    pub craftable: Option<bool>,
    /// Halloween Spells (e.g. "Exorcism", "Pumpkin Bombs") — a real,
    /// structured field on the live payload (verified live: a "BUYING
    /// SPELLED... LESS FOR NON-SPELLED" listing carried
    /// `"spells":[{"name":"Exorcism",...}]`), not something we need to
    /// text-match `details` for. Spells aren't a dimension of
    /// `domain::item::ItemKey` (Valve's own schema has no defindex/quality
    /// distinction for them either — they're a description-only attribute
    /// on the exact asset), so a spelled listing's price has no home in
    /// our per-SKU aggregation; `has_spells` is used to drop these
    /// entirely rather than silently mixing a spell premium into the
    /// plain item's buy/sell data (Module 15, found live: this was
    /// skewing Flip Finder's numbers for e.g. a 57-key spelled-only offer
    /// on an otherwise ~5-key item).
    #[serde(default)]
    pub spells: Vec<WsSpell>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsSpell {
    #[allow(dead_code)] // not surfaced anywhere yet, kept for future use/debugging
    pub name: String,
}

impl WsItem {
    pub(crate) fn has_spells(&self) -> bool {
        !self.spells.is_empty()
    }

    /// Resolves the exact `ItemKey` (matching Module 2's full domain
    /// model) — needed by `HistoryRecorder` so price history isn't
    /// contaminated by mixing e.g. a Strange Australium weapon's prices
    /// into the base weapon's trend.
    // Consumed by Module 8's history_recorder service, not wired up yet.
    #[allow(dead_code)]
    pub(crate) fn item_key(&self) -> Result<ItemKey, ItemError> {
        Ok(ItemKey {
            defindex: self.defindex,
            quality: Quality::try_from(self.quality.id)?,
            effect_id: self.particle.as_ref().map(|p| p.id),
            killstreak_tier: KillstreakTier::try_from(self.killstreak_tier.unwrap_or(0))?,
            australium: self.australium.unwrap_or(false),
            festivized: self.festivized.unwrap_or(false),
            craftable: self.craftable.unwrap_or(true),
        })
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsQuality {
    pub id: u8,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsParticle {
    pub id: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WsUser {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_spells_is_false_when_the_field_is_absent() {
        let item = WsItem {
            defindex: 45,
            quality: WsQuality { id: 6 },
            particle: None,
            killstreak_tier: None,
            australium: None,
            festivized: None,
            craftable: None,
            spells: Vec::new(),
        };
        assert!(!item.has_spells());
    }

    #[test]
    fn has_spells_is_true_when_spells_are_present() {
        let item = WsItem {
            defindex: 132,
            quality: WsQuality { id: 11 },
            particle: None,
            killstreak_tier: Some(1),
            australium: Some(true),
            festivized: None,
            craftable: Some(true),
            spells: vec![WsSpell {
                name: "Exorcism".to_string(),
            }],
        };
        assert!(item.has_spells());
    }

    #[test]
    fn parses_real_spelled_item_payload() {
        // Real shape captured live: a "BUYING SPELLED... LESS FOR
        // NON-SPELLED" listing for a Strange Killstreak Australium
        // Eyelander at ~57 keys — a price that has no relationship to a
        // non-spelled Eyelander's value (Module 15, found live).
        let json = r##"{
            "defindex": 132,
            "quality": {"id": 11, "name": "Strange", "color": "#CF6A32"},
            "killstreakTier": 1,
            "class": ["Demoman"],
            "spells": [{
                "id": "weapon-SPELL: Halloween death ghosts",
                "spellId": "SPELL: Halloween death ghosts",
                "name": "Exorcism",
                "type": "weapon"
            }],
            "slot": "melee",
            "tradable": true,
            "australium": true,
            "craftable": true
        }"##;
        let item: WsItem = serde_json::from_str(json).unwrap();
        assert!(item.has_spells());
    }

    #[test]
    fn item_key_defaults_when_optional_fields_absent() {
        let item = WsItem {
            defindex: 45,
            quality: WsQuality { id: 6 },
            particle: None,
            killstreak_tier: None,
            australium: None,
            festivized: None,
            craftable: None,
            spells: Vec::new(),
        };
        let key = item.item_key().unwrap();
        assert_eq!(key.killstreak_tier, KillstreakTier::None);
        assert!(!key.australium);
        assert!(!key.festivized);
        assert!(key.craftable);
    }

    #[test]
    fn item_key_resolves_strange_specialized_killstreak() {
        // Real shape captured live: "Strange Specialized Killstreak Quick-Fix".
        let item = WsItem {
            defindex: 411,
            quality: WsQuality { id: 11 },
            particle: None,
            killstreak_tier: Some(2),
            australium: None,
            festivized: None,
            craftable: None,
            spells: Vec::new(),
        };
        let key = item.item_key().unwrap();
        assert_eq!(key.killstreak_tier, KillstreakTier::Specialized);
    }

    #[test]
    fn item_key_resolves_strange_festivized_killstreak() {
        // Real shape captured live: "Strange Festivized Killstreak Sniper Rifle".
        let item = WsItem {
            defindex: 201,
            quality: WsQuality { id: 11 },
            particle: None,
            killstreak_tier: Some(1),
            australium: None,
            festivized: Some(true),
            craftable: Some(true),
            spells: Vec::new(),
        };
        let key = item.item_key().unwrap();
        assert_eq!(key.killstreak_tier, KillstreakTier::Killstreak);
        assert!(key.festivized);
        assert!(key.craftable);
    }

    #[test]
    fn item_key_resolves_strange_professional_killstreak_australium() {
        // Real shape captured live: "Strange Professional Killstreak Australium
        // Flame Thrower".
        let item = WsItem {
            defindex: 208,
            quality: WsQuality { id: 11 },
            particle: None,
            killstreak_tier: Some(3),
            australium: Some(true),
            festivized: None,
            craftable: Some(true),
            spells: Vec::new(),
        };
        let key = item.item_key().unwrap();
        assert_eq!(key.killstreak_tier, KillstreakTier::Professional);
        assert!(key.australium);
    }

    #[test]
    fn item_key_rejects_unknown_quality() {
        let item = WsItem {
            defindex: 1,
            quality: WsQuality { id: 99 },
            particle: None,
            killstreak_tier: None,
            australium: None,
            festivized: None,
            craftable: None,
            spells: Vec::new(),
        };
        assert!(item.item_key().is_err());
    }
}
