-- Deviation (Module 8): market_listings (Module 7) only grouped by
-- defindex+quality+effect_id — too coarse for History Recorder, which needs
-- the exact ItemKey (killstreak_tier/australium/festivized/craftable too) so
-- e.g. a Strange Australium weapon's prices don't get mixed into the base
-- weapon's history. The websocket payload already carries these fields
-- (captured live, see infra/backpack_tf/models.rs), so extending this table
-- is simpler than History Recorder maintaining a second, duplicate
-- in-memory order book.
ALTER TABLE market_listings ADD COLUMN killstreak_tier INTEGER NOT NULL DEFAULT 0;
ALTER TABLE market_listings ADD COLUMN australium INTEGER NOT NULL DEFAULT 0;
ALTER TABLE market_listings ADD COLUMN festivized INTEGER NOT NULL DEFAULT 0;
ALTER TABLE market_listings ADD COLUMN craftable INTEGER NOT NULL DEFAULT 1;

CREATE INDEX idx_market_listings_full_key
  ON market_listings(defindex, quality, effect_id, killstreak_tier, australium, festivized, craftable);
