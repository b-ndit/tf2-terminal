-- Cached inventory snapshot (docs/DESIGN.md §5). `raw_json` keeps the full
-- GetPlayerItems entry for that asset, for debugging/forward-compat.
CREATE TABLE inventory_items (
  asset_id TEXT PRIMARY KEY,
  item_id INTEGER NOT NULL REFERENCES items(id),
  steam_id TEXT NOT NULL,
  craft_number INTEGER,
  paint_id INTEGER,
  strange_count INTEGER,
  tradable INTEGER,
  marketable INTEGER,
  acquired_ts INTEGER,
  last_seen_ts INTEGER NOT NULL,
  raw_json TEXT NOT NULL
);
CREATE INDEX idx_inventory_items_steam_id ON inventory_items(steam_id);
