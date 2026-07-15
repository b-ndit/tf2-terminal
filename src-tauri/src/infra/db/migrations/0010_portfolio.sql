-- Module 12: PortfolioService's valuation history and the completed-trade
-- ledger. Tables match docs/DESIGN.md section 5 exactly; the steam_id
-- index is an implementation addition (not in the documented schema) for
-- PortfolioService's per-account history/P/L-window lookups.
CREATE TABLE portfolio_snapshots (
  ts INTEGER PRIMARY KEY,
  steam_id TEXT NOT NULL,
  total_ref REAL NOT NULL, total_keys REAL NOT NULL,
  pure_keys INTEGER, pure_metal_ref REAL,
  item_count INTEGER, unusual_count INTEGER, australium_count INTEGER
);
CREATE INDEX idx_portfolio_snapshots_steam_id ON portfolio_snapshots(steam_id, ts);

CREATE TABLE trades (
  trade_offer_id TEXT PRIMARY KEY,
  partner_steam_id TEXT NOT NULL,
  completed_ts INTEGER NOT NULL,
  given_json TEXT NOT NULL,
  received_json TEXT NOT NULL,
  net_value_ref REAL NOT NULL,
  rating INTEGER, notes TEXT
);
CREATE INDEX idx_trades_completed_ts ON trades(completed_ts);
