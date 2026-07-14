-- Foundation-level infrastructure table only. Domain tables (items,
-- price_points, inventory, etc. — see docs/DESIGN.md section 5) land in
-- Module 2+ as their owning features are built.

-- Generic KV cache with TTL, used by the infra cache layer for schema
-- blobs, image metadata, and other short-lived lookups.
CREATE TABLE kv_cache (
  key TEXT PRIMARY KEY,
  value BLOB NOT NULL,
  expires_ts INTEGER
);
