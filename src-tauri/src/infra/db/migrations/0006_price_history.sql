-- Our self-built price history (docs/DESIGN.md §5). `source` distinguishes
-- how an observation was captured: periodic snapshots of our own
-- market_listings state ('snapshot'), or the community price catalog sync
-- ('schema'). See DESIGN.md §6's HistoryRecorder note for why there's no
-- literal per-websocket-event ('ws') row.
CREATE TABLE price_points (
  id INTEGER PRIMARY KEY,
  item_id INTEGER NOT NULL REFERENCES items(id),
  ts INTEGER NOT NULL,
  source TEXT NOT NULL,
  best_buy_keys REAL, best_buy_ref REAL,
  best_sell_keys REAL, best_sell_ref REAL,
  buy_count INTEGER, sell_count INTEGER,
  key_rate_ref REAL NOT NULL
);
CREATE INDEX idx_pp_item_ts ON price_points(item_id, ts);

-- Rolled-up daily bars (computed by history_recorder, keeps charts fast).
CREATE TABLE price_daily (
  item_id INTEGER NOT NULL REFERENCES items(id),
  day INTEGER NOT NULL,
  open_ref REAL, high_ref REAL, low_ref REAL, close_ref REAL,
  avg_ref REAL, median_ref REAL, samples INTEGER,
  PRIMARY KEY (item_id, day)
);
