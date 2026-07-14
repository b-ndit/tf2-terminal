-- Item identity: one row per unique SKU permutation (docs/DESIGN.md §5).
--
-- Deviation from the doc's literal UNIQUE(...): SQLite treats every NULL in
-- a UNIQUE constraint as distinct from every other NULL, so a bare
-- UNIQUE(defindex, quality, effect_id, ...) would let a duplicate row get
-- inserted on every get_or_create call for a non-Unusual item (effect_id
-- NULL). effect_id_key coalesces NULL to a sentinel so the uniqueness check
-- actually holds, while effect_id itself stays nullable for real reads.
CREATE TABLE items (
  id INTEGER PRIMARY KEY,
  defindex INTEGER NOT NULL,
  name TEXT NOT NULL,
  quality INTEGER NOT NULL,
  effect_id INTEGER,
  effect_id_key INTEGER GENERATED ALWAYS AS (COALESCE(effect_id, -1)) VIRTUAL,
  killstreak_tier INTEGER NOT NULL DEFAULT 0,
  australium INTEGER NOT NULL DEFAULT 0,
  festivized INTEGER NOT NULL DEFAULT 0,
  craftable INTEGER NOT NULL DEFAULT 1,
  UNIQUE(defindex, quality, effect_id_key, killstreak_tier,
         australium, festivized, craftable)
);
CREATE INDEX idx_items_name ON items(name);
