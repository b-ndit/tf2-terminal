use serde::{Deserialize, Serialize};
use specta::Type;

/// TF2 item quality, numeric ids as returned by Valve's schema
/// (`IEconItems_440/GetSchemaOverview`). Ids 2 and 4 exist in the schema
/// (`rarity2`, `rarity3`) but were never assigned an item and don't appear
/// in practice — kept here so `TryFrom<u8>` stays total over the real range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[repr(u8)]
pub enum Quality {
    Normal = 0,
    Genuine = 1,
    Rarity2 = 2,
    Vintage = 3,
    Rarity3 = 4,
    Unusual = 5,
    Unique = 6,
    Community = 7,
    Valve = 8,
    SelfMade = 9,
    Customized = 10,
    Strange = 11,
    Completed = 12,
    Haunted = 13,
    Collectors = 14,
    DecoratedWeapon = 15,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ItemError {
    #[error("unknown quality id: {0}")]
    UnknownQuality(u8),
    #[error("unknown killstreak tier: {0}")]
    UnknownKillstreakTier(u8),
    #[error("invalid SKU '{0}': {1}")]
    InvalidSku(String, &'static str),
}

impl Quality {
    pub fn display_name(&self) -> &'static str {
        match self {
            Quality::Normal => "Normal",
            Quality::Genuine => "Genuine",
            Quality::Rarity2 => "rarity2",
            Quality::Vintage => "Vintage",
            Quality::Rarity3 => "rarity3",
            Quality::Unusual => "Unusual",
            Quality::Unique => "Unique",
            Quality::Community => "Community",
            Quality::Valve => "Valve",
            Quality::SelfMade => "Self-Made",
            Quality::Customized => "Customized",
            Quality::Strange => "Strange",
            Quality::Completed => "Completed",
            Quality::Haunted => "Haunted",
            Quality::Collectors => "Collector's",
            Quality::DecoratedWeapon => "Decorated Weapon",
        }
    }
}

impl TryFrom<u8> for Quality {
    type Error = ItemError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Quality::Normal),
            1 => Ok(Quality::Genuine),
            2 => Ok(Quality::Rarity2),
            3 => Ok(Quality::Vintage),
            4 => Ok(Quality::Rarity3),
            5 => Ok(Quality::Unusual),
            6 => Ok(Quality::Unique),
            7 => Ok(Quality::Community),
            8 => Ok(Quality::Valve),
            9 => Ok(Quality::SelfMade),
            10 => Ok(Quality::Customized),
            11 => Ok(Quality::Strange),
            12 => Ok(Quality::Completed),
            13 => Ok(Quality::Haunted),
            14 => Ok(Quality::Collectors),
            15 => Ok(Quality::DecoratedWeapon),
            other => Err(ItemError::UnknownQuality(other)),
        }
    }
}

impl From<Quality> for u8 {
    fn from(value: Quality) -> Self {
        value as u8
    }
}

/// Killstreak sheen/kit tier. `None` is the common case (most items aren't
/// killstreak at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Type)]
#[repr(u8)]
pub enum KillstreakTier {
    #[default]
    None = 0,
    Killstreak = 1,
    Specialized = 2,
    Professional = 3,
}

impl TryFrom<u8> for KillstreakTier {
    type Error = ItemError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(KillstreakTier::None),
            1 => Ok(KillstreakTier::Killstreak),
            2 => Ok(KillstreakTier::Specialized),
            3 => Ok(KillstreakTier::Professional),
            other => Err(ItemError::UnknownKillstreakTier(other)),
        }
    }
}

impl From<KillstreakTier> for u8 {
    fn from(value: KillstreakTier) -> Self {
        value as u8
    }
}

/// Identifies one unique SKU permutation — exactly the fields that make up
/// the `items` table's uniqueness constraint (`docs/DESIGN.md` §5).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct ItemKey {
    pub defindex: u32,
    pub quality: Quality,
    pub effect_id: Option<u32>,
    pub killstreak_tier: KillstreakTier,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
}

impl ItemKey {
    /// Canonical string form: `{defindex};{quality}` plus any modifiers that
    /// apply, e.g. `"30469;5;u18"` (an Unusual with effect 18) or
    /// `"5021;6;uncraftable"`.
    pub fn to_sku(&self) -> String {
        let mut parts = vec![self.defindex.to_string(), (self.quality as u8).to_string()];
        if !self.craftable {
            parts.push("uncraftable".to_string());
        }
        if self.australium {
            parts.push("australium".to_string());
        }
        if self.festivized {
            parts.push("festive".to_string());
        }
        if self.killstreak_tier != KillstreakTier::None {
            parts.push(format!("kt-{}", self.killstreak_tier as u8));
        }
        if let Some(effect) = self.effect_id {
            parts.push(format!("u{effect}"));
        }
        parts.join(";")
    }

    // Consumed by Module 7's classified-URL/SKU search; unit-tested here in
    // the meantime.
    #[allow(dead_code)]
    pub fn parse_sku(sku: &str) -> Result<Self, ItemError> {
        let mut segments = sku.split(';');

        let defindex = segments
            .next()
            .filter(|s| !s.is_empty())
            .ok_or(ItemError::InvalidSku(sku.to_string(), "missing defindex"))?
            .parse::<u32>()
            .map_err(|_| ItemError::InvalidSku(sku.to_string(), "invalid defindex"))?;

        let quality_id = segments
            .next()
            .ok_or(ItemError::InvalidSku(sku.to_string(), "missing quality"))?
            .parse::<u8>()
            .map_err(|_| ItemError::InvalidSku(sku.to_string(), "invalid quality"))?;
        let quality = Quality::try_from(quality_id)
            .map_err(|_| ItemError::InvalidSku(sku.to_string(), "unknown quality"))?;

        let mut craftable = true;
        let mut australium = false;
        let mut festivized = false;
        let mut killstreak_tier = KillstreakTier::None;
        let mut effect_id = None;

        for modifier in segments {
            if modifier == "uncraftable" {
                craftable = false;
            } else if modifier == "australium" {
                australium = true;
            } else if modifier == "festive" {
                festivized = true;
            } else if let Some(tier) = modifier.strip_prefix("kt-") {
                let tier = tier.parse::<u8>().map_err(|_| {
                    ItemError::InvalidSku(sku.to_string(), "invalid killstreak tier")
                })?;
                killstreak_tier = KillstreakTier::try_from(tier).map_err(|_| {
                    ItemError::InvalidSku(sku.to_string(), "unknown killstreak tier")
                })?;
            } else if let Some(effect) = modifier.strip_prefix('u') {
                effect_id =
                    Some(effect.parse::<u32>().map_err(|_| {
                        ItemError::InvalidSku(sku.to_string(), "invalid effect id")
                    })?);
            } else {
                return Err(ItemError::InvalidSku(sku.to_string(), "unknown modifier"));
            }
        }

        Ok(ItemKey {
            defindex,
            quality,
            effect_id,
            killstreak_tier,
            australium,
            festivized,
            craftable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_round_trips_through_u8_for_all_known_ids() {
        for id in 0..=15u8 {
            let quality = Quality::try_from(id).unwrap();
            assert_eq!(u8::from(quality), id);
        }
    }

    #[test]
    fn quality_rejects_out_of_range_id() {
        assert_eq!(Quality::try_from(16), Err(ItemError::UnknownQuality(16)));
    }

    #[test]
    fn killstreak_tier_round_trips() {
        for id in 0..=3u8 {
            let tier = KillstreakTier::try_from(id).unwrap();
            assert_eq!(u8::from(tier), id);
        }
        assert_eq!(
            KillstreakTier::try_from(4),
            Err(ItemError::UnknownKillstreakTier(4))
        );
    }

    fn plain_key(defindex: u32, quality: Quality) -> ItemKey {
        ItemKey {
            defindex,
            quality,
            effect_id: None,
            killstreak_tier: KillstreakTier::None,
            australium: false,
            festivized: false,
            craftable: true,
        }
    }

    #[test]
    fn sku_round_trips_for_plain_unique() {
        let key = plain_key(5021, Quality::Unique);
        assert_eq!(key.to_sku(), "5021;6");
        assert_eq!(ItemKey::parse_sku(&key.to_sku()).unwrap(), key);
    }

    #[test]
    fn sku_round_trips_for_unusual_with_effect() {
        let key = ItemKey {
            effect_id: Some(18),
            ..plain_key(30469, Quality::Unusual)
        };
        assert_eq!(key.to_sku(), "30469;5;u18");
        assert_eq!(ItemKey::parse_sku(&key.to_sku()).unwrap(), key);
    }

    #[test]
    fn sku_round_trips_for_uncraftable() {
        let key = ItemKey {
            craftable: false,
            ..plain_key(5021, Quality::Unique)
        };
        assert_eq!(key.to_sku(), "5021;6;uncraftable");
        assert_eq!(ItemKey::parse_sku(&key.to_sku()).unwrap(), key);
    }

    #[test]
    fn sku_round_trips_for_australium_festivized_killstreak() {
        let key = ItemKey {
            australium: true,
            festivized: true,
            killstreak_tier: KillstreakTier::Professional,
            ..plain_key(593, Quality::Strange)
        };
        assert_eq!(key.to_sku(), "593;11;australium;festive;kt-3");
        assert_eq!(ItemKey::parse_sku(&key.to_sku()).unwrap(), key);
    }

    #[test]
    fn sku_round_trips_for_all_modifiers_combined() {
        let key = ItemKey {
            defindex: 30743,
            quality: Quality::Unusual,
            effect_id: Some(701),
            killstreak_tier: KillstreakTier::Specialized,
            australium: true,
            festivized: true,
            craftable: false,
        };
        let sku = key.to_sku();
        assert_eq!(sku, "30743;5;uncraftable;australium;festive;kt-2;u701");
        assert_eq!(ItemKey::parse_sku(&sku).unwrap(), key);
    }

    #[test]
    fn parse_sku_rejects_missing_quality() {
        assert!(ItemKey::parse_sku("5021").is_err());
    }

    #[test]
    fn parse_sku_rejects_unknown_modifier() {
        assert!(ItemKey::parse_sku("5021;6;bogus").is_err());
    }

    #[test]
    fn parse_sku_rejects_unknown_quality_id() {
        assert!(ItemKey::parse_sku("5021;99").is_err());
    }

    #[test]
    fn parse_sku_rejects_non_numeric_defindex() {
        assert!(ItemKey::parse_sku("abc;6").is_err());
    }
}
