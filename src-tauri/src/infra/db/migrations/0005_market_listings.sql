-- Our own current-state cache of live backpack.tf listings (docs/DESIGN.md
-- §5, Module 7 addition). Upserted on New/Updated, deleted on Removed — not
-- a historical table (that's price_points/price_daily, Module 8).
CREATE TABLE market_listings (
  listing_id TEXT PRIMARY KEY,
  defindex INTEGER NOT NULL,
  quality INTEGER NOT NULL,
  effect_id INTEGER,
  intent TEXT NOT NULL,
  price_ref REAL NOT NULL,
  steam_id TEXT NOT NULL,
  steam_name TEXT,
  updated_at INTEGER NOT NULL
);
CREATE INDEX idx_market_listings_item ON market_listings(defindex, quality, effect_id);
