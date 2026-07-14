use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Minimum valid SteamID64 for an individual account (universe 1, account
/// type 1) — `76561197960265728` in decimal. Anything below this isn't a
/// real user account.
const MIN_INDIVIDUAL_STEAM_ID64: u64 = 76_561_197_960_265_728;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SteamIdError {
    #[error("'{0}' is not a valid SteamID64")]
    OutOfRange(u64),
    #[error("could not find a SteamID64 in claimed_id '{0}'")]
    UnparseableClaimedId(String),
}

/// A validated SteamID64. Public/non-secret — this is an identifier, not a
/// credential (see `docs/DESIGN.md` §2: OpenID login yields only this).
///
/// SteamID64 values (~7.6e16) exceed JS's safe integer range
/// (`Number.MAX_SAFE_INTEGER` = 2^53-1), and separately, Specta forbids
/// exporting `u64` to TypeScript to avoid silent precision loss. Both point
/// to the same fix: serialize as a string across the IPC boundary, so
/// `Serialize`/`Deserialize`/`Type` are implemented by hand below instead of
/// derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SteamId64(u64);

impl Serialize for SteamId64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for SteamId64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let id: u64 = s.parse().map_err(D::Error::custom)?;
        SteamId64::new(id).map_err(D::Error::custom)
    }
}

impl specta::Type for SteamId64 {
    fn definition(_types: &mut specta::Types) -> specta::datatype::DataType {
        specta::datatype::Primitive::str.into()
    }
}

impl SteamId64 {
    pub fn new(id: u64) -> Result<Self, SteamIdError> {
        if id >= MIN_INDIVIDUAL_STEAM_ID64 {
            Ok(Self(id))
        } else {
            Err(SteamIdError::OutOfRange(id))
        }
    }

    // Consumed by future Steam2/Steam3 id-format conversions; unit-tested
    // here in the meantime.
    #[allow(dead_code)]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Builds a SteamID64 from a 32-bit account id (e.g. a trade offer's
    /// `accountid_other`) — always in the valid individual-account range
    /// since `MIN_INDIVIDUAL_STEAM_ID64 + account_id` can't underflow the
    /// floor `new()` checks for.
    pub fn from_account_id(account_id: u32) -> Self {
        Self(MIN_INDIVIDUAL_STEAM_ID64 + account_id as u64)
    }

    /// Parses the trailing numeric segment out of an OpenID `claimed_id`
    /// URL, e.g. `"https://steamcommunity.com/openid/id/76561198000000000"`.
    pub fn parse_claimed_id(claimed_id: &str) -> Result<Self, SteamIdError> {
        let id_str = claimed_id.trim_end_matches('/').rsplit('/').next();
        let id = id_str
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| SteamIdError::UnparseableClaimedId(claimed_id.to_string()))?;
        Self::new(id)
    }
}

impl fmt::Display for SteamId64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_valid_individual_id() {
        assert!(SteamId64::new(76_561_198_000_000_000).is_ok());
    }

    #[test]
    fn new_rejects_below_individual_range() {
        assert_eq!(SteamId64::new(12345), Err(SteamIdError::OutOfRange(12345)));
    }

    #[test]
    fn from_account_id_builds_a_valid_individual_id() {
        let id = SteamId64::from_account_id(39_827_271);
        assert_eq!(id.as_u64(), MIN_INDIVIDUAL_STEAM_ID64 + 39_827_271);
    }

    #[test]
    fn parse_claimed_id_extracts_trailing_id() {
        let id =
            SteamId64::parse_claimed_id("https://steamcommunity.com/openid/id/76561198000000000")
                .unwrap();
        assert_eq!(id.as_u64(), 76_561_198_000_000_000);
    }

    #[test]
    fn parse_claimed_id_tolerates_trailing_slash() {
        let id =
            SteamId64::parse_claimed_id("https://steamcommunity.com/openid/id/76561198000000000/")
                .unwrap();
        assert_eq!(id.as_u64(), 76_561_198_000_000_000);
    }

    #[test]
    fn parse_claimed_id_rejects_non_numeric_tail() {
        assert!(SteamId64::parse_claimed_id("https://steamcommunity.com/openid/id/abc").is_err());
    }

    #[test]
    fn parse_claimed_id_rejects_garbage() {
        assert!(SteamId64::parse_claimed_id("not a url").is_err());
    }

    #[test]
    fn display_shows_plain_digits() {
        let id = SteamId64::new(76_561_198_000_000_000).unwrap();
        assert_eq!(id.to_string(), "76561198000000000");
    }

    #[test]
    fn serializes_as_a_json_string_not_a_number() {
        let id = SteamId64::new(76_561_198_000_000_000).unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"76561198000000000\"");
    }

    #[test]
    fn deserializes_from_a_json_string() {
        let id: SteamId64 = serde_json::from_str("\"76561198000000000\"").unwrap();
        assert_eq!(id.as_u64(), 76_561_198_000_000_000);
    }

    #[test]
    fn deserialize_rejects_out_of_range_value() {
        let result: Result<SteamId64, _> = serde_json::from_str("\"12345\"");
        assert!(result.is_err());
    }
}
