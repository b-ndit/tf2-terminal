-- Module 10: AlertService's rule engine. Tables match docs/DESIGN.md
-- section 5 exactly; the two indexes are an implementation addition (not
-- in the documented schema) for the lookups AlertService actually does —
-- "every enabled rule for item X" per incoming ListingEvent, and "every
-- event for rule X" for the recent-alerts feed.
CREATE TABLE alert_rules (
  id INTEGER PRIMARY KEY,
  item_id INTEGER REFERENCES items(id),
  kind TEXT NOT NULL,       -- price_drop|spread_widen|new_buyer|new_seller|hist_low|hist_high
  threshold REAL,
  channels TEXT NOT NULL,   -- json: ["desktop","discord","sound"]
  enabled INTEGER DEFAULT 1
);
CREATE INDEX idx_alert_rules_item ON alert_rules(item_id);

CREATE TABLE alert_events (
  id INTEGER PRIMARY KEY, rule_id INTEGER REFERENCES alert_rules(id),
  fired_ts INTEGER NOT NULL, payload TEXT NOT NULL, acked INTEGER DEFAULT 0
);
CREATE INDEX idx_alert_events_rule ON alert_events(rule_id);
