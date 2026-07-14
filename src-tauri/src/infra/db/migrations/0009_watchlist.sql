-- Module 11: FlipFinder's candidate universe includes watched items
-- (docs/DESIGN.md section 5). Table matches the documented schema exactly,
-- plus a UNIQUE(item_id) constraint — an implementation addition (not in
-- the schema sketch) since watching the same item twice is a pure bug
-- generator, same "documented implementation detail" precedent as the
-- indexes Modules 7/10 added beyond their own documented schemas.
CREATE TABLE watchlist (
  id INTEGER PRIMARY KEY,
  item_id INTEGER NOT NULL REFERENCES items(id),
  added_ts INTEGER NOT NULL,
  UNIQUE(item_id)
);
