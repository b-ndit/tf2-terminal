//! Pure `plugin.toml` parsing/validation for Module 14's plugin system —
//! zero I/O, per `docs/DESIGN.md` §8/§11. The WASM runtime that actually
//! executes a plugin lives in `infra::plugins` (real I/O: compiling wasm,
//! calling host functions); this module only decides whether a manifest
//! is well-formed and what capabilities/events it's asking for.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    #[error("invalid plugin.toml: {0}")]
    InvalidToml(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid plugin name: {0}")]
    InvalidName(String),
    #[error("unknown capability: {0}")]
    UnknownCapability(String),
    #[error("unknown event: {0}")]
    UnknownEvent(String),
}

/// A single granted/requested permission, per `docs/DESIGN.md` §8's
/// `market:read` / `inventory:read` / `notify:send` / `http:{domains}`.
/// Every `host_*` function in `infra::plugins::host_functions` checks the
/// plugin's granted set against exactly one of these before doing
/// anything — this enum *is* the capability boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    MarketRead,
    InventoryRead,
    NotifySend,
    /// Allowlisted domains an `http:fetch` call may reach.
    Http(Vec<String>),
}

impl Capability {
    pub fn parse(raw: &str) -> Result<Self, PluginError> {
        match raw {
            "market:read" => Ok(Capability::MarketRead),
            "inventory:read" => Ok(Capability::InventoryRead),
            "notify:send" => Ok(Capability::NotifySend),
            other => {
                if let Some(domains) = other.strip_prefix("http:") {
                    let list: Vec<String> = domains
                        .split(',')
                        .map(|d| d.trim().to_string())
                        .filter(|d| !d.is_empty())
                        .collect();
                    if list.is_empty() {
                        return Err(PluginError::UnknownCapability(raw.to_string()));
                    }
                    Ok(Capability::Http(list))
                } else {
                    Err(PluginError::UnknownCapability(raw.to_string()))
                }
            }
        }
    }

    /// Round-trips back to the manifest string form — used both to
    /// redisplay a plugin's requested capabilities to the user and to
    /// persist them as `capabilities_json` in the `plugins` table.
    pub fn as_string(&self) -> String {
        match self {
            Capability::MarketRead => "market:read".to_string(),
            Capability::InventoryRead => "inventory:read".to_string(),
            Capability::NotifySend => "notify:send".to_string(),
            Capability::Http(domains) => format!("http:{}", domains.join(",")),
        }
    }

    /// Whether an `http:fetch(url)` call is allowed to reach `host` under
    /// this capability (only meaningful for the `Http` variant).
    pub fn allows_http_host(&self, host: &str) -> bool {
        matches!(self, Capability::Http(domains) if domains.iter().any(|d| d == host))
    }
}

/// A declared subscription to a host-driven callback — whether the plugin
/// wants `on_alert_fired` invoked (acting as a notification channel) and/or
/// `provide_listings` polled (acting as a market data provider). Declaring
/// interest here is separate from `Capability`: an event subscription says
/// *when* the host calls into the plugin, a capability says *what* the
/// plugin is allowed to do once called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginEvent {
    AlertFired,
    MarketProvider,
}

impl PluginEvent {
    pub fn parse(raw: &str) -> Result<Self, PluginError> {
        match raw {
            "alert_fired" => Ok(PluginEvent::AlertFired),
            "market_provider" => Ok(PluginEvent::MarketProvider),
            other => Err(PluginError::UnknownEvent(other.to_string())),
        }
    }

    pub fn as_string(&self) -> &'static str {
        match self {
            PluginEvent::AlertFired => "alert_fired",
            PluginEvent::MarketProvider => "market_provider",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub entry: String,
    pub capabilities: Vec<Capability>,
    pub events: Vec<PluginEvent>,
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    name: String,
    version: String,
    entry: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    events: Vec<String>,
}

/// Sanitizes a plugin name for use as a filesystem directory name under
/// `data_dir/plugins/<name>/` — this is a real security boundary, not
/// cosmetic: an unsanitized name (`"../../etc"`, `"/etc/passwd"`) would let
/// a malicious `plugin.toml` write/read outside the plugins directory.
/// Restricting to ASCII alphanumeric/`-`/`_` rules out `.`, `/`, and `\`
/// entirely, so path traversal is impossible by construction rather than
/// by denylisting specific patterns.
pub fn sanitize_plugin_name(name: &str) -> Result<String, PluginError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(PluginError::InvalidName("name is empty".to_string()));
    }
    if trimmed.len() > 64 {
        return Err(PluginError::InvalidName(
            "name is longer than 64 characters".to_string(),
        ));
    }
    let valid = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(PluginError::InvalidName(format!(
            "'{trimmed}' may only contain letters, digits, '-', and '_'"
        )));
    }
    Ok(trimmed.to_string())
}

/// Parses and validates a `plugin.toml` document. Pure: no filesystem
/// access, doesn't check that `entry` actually exists on disk (that's
/// `services::plugin_service::install_plugin`'s job, since it's I/O).
pub fn parse_manifest(toml_str: &str) -> Result<PluginManifest, PluginError> {
    let raw: RawManifest =
        toml::from_str(toml_str).map_err(|e| PluginError::InvalidToml(e.to_string()))?;

    let name = sanitize_plugin_name(&raw.name)?;
    if raw.version.trim().is_empty() {
        return Err(PluginError::MissingField("version"));
    }
    if raw.entry.trim().is_empty() {
        return Err(PluginError::MissingField("entry"));
    }

    let capabilities = raw
        .capabilities
        .iter()
        .map(|s| Capability::parse(s))
        .collect::<Result<Vec<_>, _>>()?;
    let events = raw
        .events
        .iter()
        .map(|s| PluginEvent::parse(s))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PluginManifest {
        name,
        version: raw.version,
        entry: raw.entry,
        capabilities,
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_manifest() {
        let toml = r#"
            name = "sample-plugin"
            version = "0.1.0"
            entry = "plugin.wasm"
            capabilities = ["market:read", "notify:send"]
            events = ["alert_fired"]
        "#;
        let manifest = parse_manifest(toml).unwrap();
        assert_eq!(manifest.name, "sample-plugin");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.entry, "plugin.wasm");
        assert_eq!(
            manifest.capabilities,
            vec![Capability::MarketRead, Capability::NotifySend]
        );
        assert_eq!(manifest.events, vec![PluginEvent::AlertFired]);
    }

    #[test]
    fn parses_a_manifest_with_no_capabilities_or_events() {
        let toml = r#"
            name = "minimal"
            version = "1.0.0"
            entry = "plugin.wasm"
        "#;
        let manifest = parse_manifest(toml).unwrap();
        assert!(manifest.capabilities.is_empty());
        assert!(manifest.events.is_empty());
    }

    #[test]
    fn parses_an_http_capability_with_multiple_domains() {
        let toml = r#"
            name = "http-plugin"
            version = "1.0.0"
            entry = "plugin.wasm"
            capabilities = ["http:api.example.com, cdn.example.com"]
        "#;
        let manifest = parse_manifest(toml).unwrap();
        assert_eq!(
            manifest.capabilities,
            vec![Capability::Http(vec![
                "api.example.com".to_string(),
                "cdn.example.com".to_string()
            ])]
        );
    }

    #[test]
    fn rejects_invalid_toml() {
        let err = parse_manifest("not valid toml {{{").unwrap_err();
        assert!(matches!(err, PluginError::InvalidToml(_)));
    }

    #[test]
    fn rejects_a_missing_version() {
        let toml = r#"
            name = "no-version"
            version = ""
            entry = "plugin.wasm"
        "#;
        let err = parse_manifest(toml).unwrap_err();
        assert_eq!(err, PluginError::MissingField("version"));
    }

    #[test]
    fn rejects_an_unknown_capability_string() {
        let toml = r#"
            name = "bad-cap"
            version = "1.0.0"
            entry = "plugin.wasm"
            capabilities = ["filesystem:write"]
        "#;
        let err = parse_manifest(toml).unwrap_err();
        assert!(matches!(err, PluginError::UnknownCapability(_)));
    }

    #[test]
    fn rejects_an_unknown_event_string() {
        let toml = r#"
            name = "bad-event"
            version = "1.0.0"
            entry = "plugin.wasm"
            events = ["on_startup"]
        "#;
        let err = parse_manifest(toml).unwrap_err();
        assert!(matches!(err, PluginError::UnknownEvent(_)));
    }

    #[test]
    fn sanitize_accepts_a_normal_name() {
        assert_eq!(
            sanitize_plugin_name("sample-plugin_v2").unwrap(),
            "sample-plugin_v2"
        );
    }

    #[test]
    fn sanitize_rejects_an_empty_name() {
        assert!(matches!(
            sanitize_plugin_name("   "),
            Err(PluginError::InvalidName(_))
        ));
    }

    #[test]
    fn sanitize_rejects_path_traversal_attempts() {
        assert!(matches!(
            sanitize_plugin_name("../../etc"),
            Err(PluginError::InvalidName(_))
        ));
        assert!(matches!(
            sanitize_plugin_name("/etc/passwd"),
            Err(PluginError::InvalidName(_))
        ));
        assert!(matches!(
            sanitize_plugin_name("..\\..\\windows"),
            Err(PluginError::InvalidName(_))
        ));
    }

    #[test]
    fn sanitize_rejects_a_name_over_64_characters() {
        let long_name = "a".repeat(65);
        assert!(matches!(
            sanitize_plugin_name(&long_name),
            Err(PluginError::InvalidName(_))
        ));
    }

    #[test]
    fn capability_round_trips_through_its_string_form() {
        for cap in [
            Capability::MarketRead,
            Capability::InventoryRead,
            Capability::NotifySend,
            Capability::Http(vec!["a.com".to_string(), "b.com".to_string()]),
        ] {
            let s = cap.as_string();
            assert_eq!(Capability::parse(&s).unwrap(), cap);
        }
    }

    #[test]
    fn http_capability_checks_the_allowlisted_domain_exactly() {
        let cap = Capability::Http(vec!["api.example.com".to_string()]);
        assert!(cap.allows_http_host("api.example.com"));
        assert!(!cap.allows_http_host("evil.com"));
        assert!(!Capability::MarketRead.allows_http_host("api.example.com"));
    }
}
