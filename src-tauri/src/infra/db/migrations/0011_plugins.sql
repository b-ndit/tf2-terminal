-- Module 14: installed plugin metadata. `capabilities_json`/`events_json`
-- store the manifest's parsed `Capability`/`PluginEvent` string forms
-- (domain::plugin::{Capability,PluginEvent}::as_string) as a JSON array,
-- so re-reading a row never needs to re-parse plugin.toml from disk.
CREATE TABLE plugins (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  version TEXT NOT NULL,
  entry_file TEXT NOT NULL,
  capabilities_json TEXT NOT NULL,
  events_json TEXT NOT NULL,
  has_panel INTEGER NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  installed_ts INTEGER NOT NULL
);
