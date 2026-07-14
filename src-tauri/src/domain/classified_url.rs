//! Parses a backpack.tf classifieds search URL (e.g. one a user copies from
//! their browser while browsing listings) into a structured query. Pure —
//! `url::Url::parse` is plain string parsing, no network access.
//!
//! Query parameter names match backpack.tf's long-stable, widely-documented
//! convention (used by numerous community tools): `item`, `quality`,
//! `particle`, `tradable`, `craftable`, `australium`, `killstreak_tier`,
//! `paint`.

use std::collections::HashMap;

use url::Url;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClassifiedUrlError {
    #[error("could not parse URL: {0}")]
    InvalidUrl(String),
    #[error("missing required 'item' query parameter")]
    MissingItem,
    #[error("missing required 'quality' query parameter")]
    MissingQuality,
    #[error("invalid quality value: '{0}'")]
    InvalidQuality(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifiedQuery {
    pub item_name: String,
    pub quality: u8,
    pub particle: Option<u32>,
    pub tradable: Option<bool>,
    pub craftable: Option<bool>,
    /// `None` covers both "not specified" and backpack.tf's own `-1`
    /// ("any") sentinel — both mean "don't filter on this field".
    pub australium: Option<bool>,
    pub killstreak_tier: Option<u8>,
    pub paint: Option<u32>,
}

pub fn parse_classified_url(input: &str) -> Result<ClassifiedQuery, ClassifiedUrlError> {
    let url = Url::parse(input)
        .or_else(|_| Url::parse(&format!("https://{input}")))
        .map_err(|e| ClassifiedUrlError::InvalidUrl(e.to_string()))?;

    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

    let item_name = params
        .get("item")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or(ClassifiedUrlError::MissingItem)?;

    let quality_str = params
        .get("quality")
        .ok_or(ClassifiedUrlError::MissingQuality)?;
    let quality = quality_str
        .parse::<u8>()
        .map_err(|_| ClassifiedUrlError::InvalidQuality(quality_str.clone()))?;

    Ok(ClassifiedQuery {
        item_name,
        quality,
        particle: params.get("particle").and_then(|s| s.parse::<u32>().ok()),
        tradable: params.get("tradable").and_then(|s| parse_bool_flag(s)),
        craftable: params.get("craftable").and_then(|s| parse_bool_flag(s)),
        australium: params.get("australium").and_then(|s| parse_bool_flag(s)),
        killstreak_tier: params
            .get("killstreak_tier")
            .and_then(|s| s.parse::<u8>().ok()),
        paint: params.get("paint").and_then(|s| s.parse::<u32>().ok()),
    })
}

/// `1` -> true, `0` -> false, anything else (including backpack.tf's `-1`
/// "any" sentinel) -> `None`.
fn parse_bool_flag(s: &str) -> Option<bool> {
    match s {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_real_shaped_url() {
        let url = "https://backpack.tf/classifieds?item=Team+Captain&quality=5&tradable=1&craftable=1&australium=0&killstreak_tier=0&particle=13";
        let q = parse_classified_url(url).unwrap();
        assert_eq!(q.item_name, "Team Captain");
        assert_eq!(q.quality, 5);
        assert_eq!(q.particle, Some(13));
        assert_eq!(q.tradable, Some(true));
        assert_eq!(q.craftable, Some(true));
        assert_eq!(q.australium, Some(false));
        assert_eq!(q.killstreak_tier, Some(0));
    }

    #[test]
    fn decodes_percent_encoded_item_names() {
        let url = "https://backpack.tf/classifieds?item=Bill%27s%20Hat&quality=6";
        let q = parse_classified_url(url).unwrap();
        assert_eq!(q.item_name, "Bill's Hat");
    }

    #[test]
    fn tolerates_missing_scheme() {
        let url = "backpack.tf/classifieds?item=Team+Captain&quality=5";
        let q = parse_classified_url(url).unwrap();
        assert_eq!(q.item_name, "Team Captain");
    }

    #[test]
    fn australium_any_sentinel_maps_to_none() {
        let url = "https://backpack.tf/classifieds?item=Rocket+Launcher&quality=6&australium=-1";
        let q = parse_classified_url(url).unwrap();
        assert_eq!(q.australium, None);
    }

    #[test]
    fn minimal_url_only_needs_item_and_quality() {
        let url = "https://backpack.tf/classifieds?item=Team+Captain&quality=5";
        let q = parse_classified_url(url).unwrap();
        assert_eq!(q.particle, None);
        assert_eq!(q.tradable, None);
        assert_eq!(q.paint, None);
    }

    #[test]
    fn rejects_url_missing_item() {
        let url = "https://backpack.tf/classifieds?quality=5";
        assert_eq!(
            parse_classified_url(url),
            Err(ClassifiedUrlError::MissingItem)
        );
    }

    #[test]
    fn rejects_url_missing_quality() {
        let url = "https://backpack.tf/classifieds?item=Team+Captain";
        assert_eq!(
            parse_classified_url(url),
            Err(ClassifiedUrlError::MissingQuality)
        );
    }

    #[test]
    fn rejects_non_numeric_quality() {
        let url = "https://backpack.tf/classifieds?item=Team+Captain&quality=abc";
        assert!(matches!(
            parse_classified_url(url),
            Err(ClassifiedUrlError::InvalidQuality(_))
        ));
    }

    #[test]
    fn rejects_garbage_input() {
        assert!(parse_classified_url("not a url at all \u{0}").is_err());
    }
}
