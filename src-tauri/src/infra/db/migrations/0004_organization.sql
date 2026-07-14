-- User organization layer (docs/DESIGN.md §5). ON DELETE CASCADE added on
-- the FKs, not spelled out in the doc but clearly intended: losing an
-- inventory item (traded away) or deleting a tag should clean up its
-- associations rather than leaving orphaned rows.
CREATE TABLE item_meta (
  asset_id TEXT PRIMARY KEY REFERENCES inventory_items(asset_id) ON DELETE CASCADE,
  folder TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  favorite INTEGER NOT NULL DEFAULT 0,
  note TEXT,
  custom_label TEXT
);

CREATE TABLE tags (
  id INTEGER PRIMARY KEY,
  name TEXT UNIQUE NOT NULL,
  color TEXT NOT NULL
);

CREATE TABLE item_tags (
  asset_id TEXT NOT NULL REFERENCES inventory_items(asset_id) ON DELETE CASCADE,
  tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (asset_id, tag_id)
);
