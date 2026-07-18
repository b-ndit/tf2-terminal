# TF2 Terminal — Architecture & Design Document (Phase 0)

**Version:** 0.1 (pre-implementation)
**Status:** Awaiting approval before Module 1

---

## 1. Technology Stack Decision

### Chosen stack: Tauri 2.x + React 18 + TypeScript, Rust backend, SQLite

| Concern | Why Tauri wins |
|---|---|
| Memory | ~60–120 MB idle vs 300–500 MB for Electron. Matters for an always-on trading tool. |
| Startup | Sub-second cold start using the OS webview. |
| Backend | Rust core is the right place for the analytics engine, websocket ingestion, rate-limited HTTP clients, and SQLite access. No IPC-to-Node overhead. |
| Security | No Node integration in the renderer; capability-based permissions; secrets stay in the Rust side / OS keychain. |
| Cross-platform | Windows, macOS, Linux (WebKitGTK). Your daily drivers are Linux, so this is a hard requirement — Tauri on Fedora/Nobara works well with `webkit2gtk-4.1`. |
| Distribution | Small binaries (~10 MB), auto-updater built in. |

**Supporting choices:**

- **UI:** React 18 + TypeScript (strict), Vite build
- **State:** Zustand (app state) + TanStack Query (server/cache state) — deliberately not Redux; lower boilerplate, better async cache semantics for market data
- **Charts:** Lightweight Charts (TradingView's OSS library) for price history — purpose-built for financial charting, canvas-based, tiny
- **Layout/docking:** Dockview (dockable panels, saved layouts, multi-window) — this is what gives the "Bloomberg terminal" feel
- **Styling:** Tailwind CSS + CSS variables for themeability
- **Rust crates:** `tokio` (async runtime), `reqwest` (HTTP), `tokio-tungstenite` (backpack.tf websocket), `sqlx` (async SQLite, compile-time checked queries), `serde`, `tracing` (structured logging), `governor` (rate limiting), `keyring` (OS credential store)
- **DB:** SQLite in WAL mode. One file, zero admin, easily handles millions of price-point rows with proper indexes.
- **Testing:** `cargo test` + `insta` snapshots (Rust); Vitest + React Testing Library (frontend); Playwright for E2E smoke tests

### Rejected alternatives

- **Electron:** heavier, slower start, larger attack surface. Only advantage is Chromium consistency, which we don't need.
- **Native (egui/Qt):** faster still, but the UI complexity (docking, tooltips, rich item cards, charts) is dramatically cheaper in web tech, and the contribution pool is larger.

---

## 2. Data Source Reality Check (drives the whole design)

This is the part most TF2 tools get wrong, so it's settled before architecture.

### Backpack.tf
- **Developer Centre / v2 APIs:** backpack.tf's v1 listing APIs are deprecated and rate-limited; **v2 is the required target**. A token-authenticated REST API and an official **websocket service** exist for live listing events.
- **Classifieds snapshot endpoint** ~~(`/api/classifieds/listings/snapshot`)~~ **Deviation (Module 5):** this endpoint no longer appears in backpack.tf's published Swagger spec (`/api/swagger.json`) — only account-scoped listing endpoints are documented now. It still responds live but returns 401 without auth we couldn't verify, and forum discussion suggests it's being phased out. Verified live instead: the public **websocket** (`wss://ws.backpack.tf/events`) needs **no auth at all** and streams full `listing-update`/`listing-delete` events (complete item/price/user payloads) for every listing across the whole marketplace — a strictly richer, more "build our own" fit than the deprecated REST snapshot. **Decision: the websocket is the primary `ListingEvent` source; the snapshot endpoint is dropped.**
- **Community price schema** (`IGetPrices`) gives suggested prices for the entire item catalog.
- **Price history:** the official API's history endpoint is limited; deep per-item history is partially premium-gated. **Design decision:** we build our *own* history by continuously recording snapshots + websocket events into SQLite. Day 1 the history is shallow; it deepens the longer the app runs. Optional future plugin: Gladiator.tf sales data (with permission) or backpack.tf premium.
- **Compliance rules baked into the client layer:** token auth only, honor `Retry-After` on 429s, global rate limiter, exponential backoff, user-agent identifying the app, no scraping of pages that have API equivalents. Any HTML parsing (none planned at v1) would live behind the same `MarketDataProvider` trait so it's swappable.

### Steam
- **Auth:** Steam OpenID 2.0 via system browser → we receive only the SteamID64. **We never see credentials.** Steam Web API key (user-provided, stored in OS keychain) unlocks trade-offer endpoints. **Deviation (Module 3, found post-hoc during Module 15's packaging verification):** on WSL2, `infra::steam::auth::login_via_browser`'s system-browser launch (`open` 5.4.0) silently failed — verified live against a real WSL2 install. `open`'s WSL code path spawns `powershell.exe` with the target URL passed via an `OPEN_RS_TARGET` env var, but WSL interop only forwards env vars to spawned Windows processes that are listed in `WSLENV`, which `open` never sets; the crate is at its latest version, so this isn't a fixed-upstream issue. `login_via_browser` now sets `WSLENV` to include `OPEN_RS_TARGET` (appending to, not clobbering, any existing value) immediately before calling `open::that` — a no-op on every non-WSL platform, since nothing else reads `WSLENV`.
- **Inventory:** ~~public inventory endpoint (`steamcommunity.com/inventory/{steamid}/440/2`)~~ **Deviation (Module 3):** that endpoint's TF2 response carries no numeric defindex/quality/attribute data — only human-readable tags and description strings (e.g. `"★ Unusual Effect: Hot"` as text), which can't reliably drive the `ItemKey` model from Module 2. Verified live against a real inventory. Using `IEconItems_440/GetPlayerItems` instead — same API family/key as the schema sync, returns structured `defindex`/`quality`/`attributes` (verified live: attribute 134 = unusual effect, 2025 = killstreak tier, 2027 = australium, 2053 = festivized, `flag_cannot_craft`/`flag_cannot_trade` booleans). Still paginated key-holder-respectful, cached aggressively in `inventory_items`.
- **Trade offers:** `IEconService/GetTradeOffers` (user's own API key — officially supported for acting on your own account). Polled at a respectful interval; no trade *acceptance* automation in v1 (analysis only), which keeps us clearly inside Steam's ToS. **Deviation (found live, Module 15 verification):** the first real call against a live account failed with an opaque "error decoding response body" — Steam's Web API is inconsistent about whether large integer fields (`tradeofferid` here) come back as JSON numbers or JSON-quoted strings, and serde doesn't coerce between the two by default. `TradeOffer::tradeofferid` now accepts either representation. `SteamApiClient::get_json` also now fetches the raw body and logs a snippet on any future decode failure — the original `reqwest`-only error gave no way to see what Steam actually sent, which is what made this one hard to diagnose from the error message alone.
- **Item schema:** `IEconItems_440/GetSchemaItems` + `GetSchemaOverview` for qualities, effects, paints — cached in SQLite, refreshed weekly. **Deviation (found live, Module 15):** `GetSchemaItems`' `image_url` field was parsed from day one but silently discarded — `items` had no column for it, so the backpack grid never had real item icons to show, only names (or "Unknown Item {defindex}" whenever the schema itself had never been synced — `commands::schema::sync_item_schema` existed since Module 1 but, like the Settings secrets below, was never wired into any frontend UI until now). Migration `0012` adds `items.image_url`; `schema_service::sync` now persists it via `ItemsRepo::set_image_url`, and it flows through `InventoryItemView`/`BackpackItem` to `ItemTile`, which renders the real icon when present and falls back to the name as text otherwise.

### Currency model
All values normalized internally to refined metal: store prices as `(keys: f64, metal_refined: f64)` plus a computed `value_in_ref` using the live key↔ref rate (itself tracked as a time series). USD display derived from the key's community price.

**Addition (Module 15, requested live):** Market Analyzer displays every ref amount as "N keys, M.MM ref" — matching backpack.tf's own classifieds convention — rather than a flat ref number, via a new `get_key_rate` command (`market_analyzer_service::get_key_rate_ref`) that values the Key itself the same way `portfolio_service::snapshot_now` already does (reusing `item_valuation::value_item_key` rather than a second rate source), plus a pure `formatCurrency` helper on the frontend mirroring `domain::currency::Currency::from_total_ref`'s floor/remainder split exactly. Also cleaned up the stats grid layout while in there — a fixed `grid-cols-6` had been truncating labels/values illegibly in Dockview's narrower panel widths (verified live); it's now an `auto-fit`/`minmax` grid that reflows instead of squeezing, and the buyer/seller tables now stack full-width rather than competing for half a narrow panel each.

---

## 3. Layered Architecture

```
┌─────────────────────────────────────────────────────────┐
│ PRESENTATION (React/TS)                                  │
│  Workspaces · Dockable panels · Backpack grid · Charts   │
├─────────────────────────────────────────────────────────┤
│ IPC BOUNDARY (Tauri commands + events, typed via specta) │
├─────────────────────────────────────────────────────────┤
│ APPLICATION SERVICES (Rust)                              │
│  InventoryService · TradeAnalysisEngine · FlipFinder     │
│  AlertService · PortfolioService · SimulatorService      │
├─────────────────────────────────────────────────────────┤
│ DOMAIN / ANALYTICS ENGINE (pure Rust, no I/O)            │
│  Pricing math · Liquidity/demand scoring · Trade rating  │
│  Spread/ROI calculators · Trend detection                │
├─────────────────────────────────────────────────────────┤
│ INFRASTRUCTURE                                           │
│  BackpackTfClient (REST+WS) · SteamClient · CacheLayer   │
│  SQLite repositories · NotificationSink (OS/Discord)     │
│  PluginHost (WASM) · Config · Keychain                   │
└─────────────────────────────────────────────────────────┘
```

**Rules:**
- The **domain layer has zero I/O** — every scoring/rating function is a pure function over structs, which makes the analytics engine trivially unit-testable.
- Infrastructure implements **traits** (`MarketDataProvider`, `InventoryProvider`, `TradeOfferProvider`, `NotificationSink`) defined by the application layer — dependency inversion, so backpack.tf could be swapped or mocked.
- Frontend never talks to the network. All external I/O goes through Rust.
- Live data flows frontend-ward via Tauri **events** (push), request/response via **commands** (pull). Types generated once with `specta`/`tauri-specta` so TS and Rust never drift.

---

## 4. Folder Structure

```
tf2-terminal/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs                  # thin bootstrap only
│   │   ├── app.rs                   # DI container / service wiring
│   │   ├── commands/                # one file per feature area
│   │   │   ├── inventory.rs
│   │   │   ├── market.rs
│   │   │   ├── trades.rs
│   │   │   ├── portfolio.rs
│   │   │   ├── alerts.rs
│   │   │   └── settings.rs
│   │   ├── services/
│   │   │   ├── inventory_service.rs
│   │   │   ├── market_data_service.rs
│   │   │   ├── trade_analysis_engine.rs
│   │   │   ├── flip_finder.rs
│   │   │   ├── alert_service.rs
│   │   │   ├── portfolio_service.rs
│   │   │   ├── simulator_service.rs
│   │   │   └── history_recorder.rs   # snapshot → time-series writer
│   │   ├── domain/                   # pure logic, no I/O
│   │   │   ├── currency.rs           # keys/metal math, normalization
│   │   │   ├── item.rs               # SKU model, quality/effect/paint
│   │   │   ├── pricing.rs            # spread, median, estimators
│   │   │   ├── liquidity.rs          # liquidity & demand scoring
│   │   │   ├── trade_rating.rs       # ★ rating + explanation builder
│   │   │   └── trend.rs              # MA, volatility, change windows
│   │   ├── infra/
│   │   │   ├── backpack_tf/
│   │   │   │   ├── client.rs         # REST, rate-limited
│   │   │   │   ├── websocket.rs      # live listing events
│   │   │   │   └── models.rs
│   │   │   ├── steam/
│   │   │   │   ├── auth.rs           # OpenID flow
│   │   │   │   ├── inventory.rs
│   │   │   │   ├── trade_offers.rs
│   │   │   │   └── schema.rs
│   │   │   ├── db/
│   │   │   │   ├── migrations/       # sqlx migrations, versioned
│   │   │   │   └── repos/            # one repo per aggregate
│   │   │   ├── cache.rs              # tiered memory+SQLite cache
│   │   │   ├── notify/               # os.rs, discord.rs, sound.rs
│   │   │   ├── keychain.rs
│   │   │   └── config.rs
│   │   ├── plugins/
│   │   │   ├── host.rs               # WASM runtime (wasmtime)
│   │   │   ├── api.rs                # capability-scoped plugin API
│   │   │   └── manifest.rs
│   │   └── telemetry.rs              # tracing setup, log rotation
│   └── tauri.conf.json
├── src/                              # React frontend
│   ├── app/                          # shell, routing, workspace mgr
│   ├── features/                     # mirrors backend features
│   │   ├── backpack/                 # grid, tooltips, item panel
│   │   ├── market-analyzer/
│   │   ├── price-history/
│   │   ├── trade-analyzer/
│   │   ├── simulator/
│   │   ├── flip-finder/
│   │   ├── live-feed/
│   │   ├── alerts/
│   │   ├── portfolio/
│   │   ├── trade-history/
│   │   └── search/
│   ├── components/                   # shared UI primitives
│   ├── lib/                          # ipc client (generated), utils
│   ├── stores/                       # zustand slices
│   └── themes/
├── plugins-sdk/                      # published SDK for plugin authors
├── docs/                             # ADRs, API notes, this document
└── e2e/
```

No file should exceed ~400 lines; features are vertical slices front and back.

---

## 5. Database Schema (SQLite, sqlx migrations)

```sql
-- Item identity: one row per unique SKU permutation
CREATE TABLE items (
  id INTEGER PRIMARY KEY,
  defindex INTEGER NOT NULL,
  name TEXT NOT NULL,
  quality INTEGER NOT NULL,           -- 5=Unusual, 6=Unique, 11=Strange...
  effect_id INTEGER,                  -- unusual effect, nullable
  killstreak_tier INTEGER DEFAULT 0,
  australium INTEGER DEFAULT 0,
  festivized INTEGER DEFAULT 0,
  craftable INTEGER DEFAULT 1,
  UNIQUE(defindex, quality, effect_id, killstreak_tier,
         australium, festivized, craftable)
);
CREATE INDEX idx_items_name ON items(name);

-- Our self-built price history (append-only time series)
CREATE TABLE price_points (
  id INTEGER PRIMARY KEY,
  item_id INTEGER NOT NULL REFERENCES items(id),
  ts INTEGER NOT NULL,                -- unix seconds
  source TEXT NOT NULL,               -- 'snapshot' | 'ws' | 'schema'
  best_buy_keys REAL, best_buy_ref REAL,
  best_sell_keys REAL, best_sell_ref REAL,
  buy_count INTEGER, sell_count INTEGER,   -- market depth
  key_rate_ref REAL NOT NULL          -- key↔ref at capture time
);
CREATE INDEX idx_pp_item_ts ON price_points(item_id, ts);

-- Rolled-up daily bars (computed by history_recorder, keeps charts fast)
CREATE TABLE price_daily (
  item_id INTEGER NOT NULL REFERENCES items(id),
  day INTEGER NOT NULL,               -- unix day
  open_ref REAL, high_ref REAL, low_ref REAL, close_ref REAL,
  avg_ref REAL, median_ref REAL, samples INTEGER,
  PRIMARY KEY (item_id, day)
);

-- Cached inventory snapshot
CREATE TABLE inventory_items (
  asset_id TEXT PRIMARY KEY,
  item_id INTEGER NOT NULL REFERENCES items(id),
  steam_id TEXT NOT NULL,
  craft_number INTEGER,
  paint_id INTEGER,
  strange_count INTEGER,
  tradable INTEGER, marketable INTEGER,
  acquired_ts INTEGER,
  last_seen_ts INTEGER NOT NULL,
  raw_json TEXT NOT NULL              -- full asset+description blob
);

-- User organization layer
CREATE TABLE item_meta (
  asset_id TEXT PRIMARY KEY REFERENCES inventory_items(asset_id),
  folder TEXT, pinned INTEGER DEFAULT 0, favorite INTEGER DEFAULT 0,
  note TEXT, custom_label TEXT
);
CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT UNIQUE, color TEXT);
CREATE TABLE item_tags (
  asset_id TEXT REFERENCES inventory_items(asset_id),
  tag_id INTEGER REFERENCES tags(id),
  PRIMARY KEY (asset_id, tag_id)
);

-- Watchlist & alerts
CREATE TABLE watchlist (
  id INTEGER PRIMARY KEY,
  item_id INTEGER NOT NULL REFERENCES items(id),
  added_ts INTEGER NOT NULL
);
CREATE TABLE alert_rules (
  id INTEGER PRIMARY KEY,
  item_id INTEGER REFERENCES items(id),
  kind TEXT NOT NULL,       -- price_drop|spread_widen|new_buyer|new_seller|hist_low|hist_high
  threshold REAL,
  channels TEXT NOT NULL,   -- json: ["desktop","discord","sound"]
  enabled INTEGER DEFAULT 1
);
CREATE TABLE alert_events (
  id INTEGER PRIMARY KEY, rule_id INTEGER REFERENCES alert_rules(id),
  fired_ts INTEGER NOT NULL, payload TEXT NOT NULL, acked INTEGER DEFAULT 0
);

-- Portfolio time series (whole-backpack valuation)
CREATE TABLE portfolio_snapshots (
  ts INTEGER PRIMARY KEY,
  steam_id TEXT NOT NULL,
  total_ref REAL NOT NULL, total_keys REAL NOT NULL,
  pure_keys INTEGER, pure_metal_ref REAL,
  item_count INTEGER, unusual_count INTEGER, australium_count INTEGER
);

-- Completed trades ledger
CREATE TABLE trades (
  trade_offer_id TEXT PRIMARY KEY,
  partner_steam_id TEXT NOT NULL,
  completed_ts INTEGER NOT NULL,
  given_json TEXT NOT NULL,           -- [{item_id, asset_id, value_ref}]
  received_json TEXT NOT NULL,
  net_value_ref REAL NOT NULL,        -- valuation at completion time
  rating INTEGER, notes TEXT
);

-- Generic KV cache with TTL (schema blobs, images metadata, etc.)
CREATE TABLE kv_cache (
  key TEXT PRIMARY KEY, value BLOB NOT NULL,
  expires_ts INTEGER
);
```

Retention policy: raw `price_points` for watched/owned items kept indefinitely; unwatched items compacted into `price_daily` after 90 days.

**Addition (Module 7):** a direct consequence of Module 5's websocket-first decision — with the classifieds snapshot REST endpoint dropped, "buyers/sellers tables for item X" has no on-demand source; it must be accumulated locally from the websocket stream. `market_listings` is our own current-state cache of live listings (upserted on `New`/`Updated`, deleted on `Removed`), not a historical table:
```sql
CREATE TABLE market_listings (
  listing_id TEXT PRIMARY KEY,
  defindex INTEGER NOT NULL,
  quality INTEGER NOT NULL,
  effect_id INTEGER,
  intent TEXT NOT NULL,          -- 'buy' | 'sell'
  price_ref REAL NOT NULL,
  steam_id TEXT NOT NULL,
  steam_name TEXT,
  updated_at INTEGER NOT NULL
);
CREATE INDEX idx_market_listings_item ON market_listings(defindex, quality, effect_id);
```
Coverage deepens the longer the app runs, same "build our own" tradeoff already accepted for price history.

---

## 6. Core Services (contracts)

**MarketDataService** — owns the backpack.tf REST client, websocket consumer, and rate limiter. Emits `ListingEvent { item, side, price, kind: New|Removed|Updated }` on an internal broadcast channel. Everything downstream (live feed, flip finder, alerts, history recorder) subscribes to this one stream — single ingestion point, fan-out consumers.

**InventoryService** — OpenID login, inventory sync with diffing (only changed assets touch the DB), refresh triggers (manual, interval, post-trade), emits `InventoryChanged`.

**AnalyticsEngine (domain)** — pure functions:
- `spread(listings) -> Spread { abs_ref, pct }`
- `liquidity_score(depth, listing_ages, volume_7d) -> 0..100`
- `demand_score(buy_depth, buy_growth, sale_velocity) -> 0..100`
- `estimate_sale_price(listings, history) -> ref` (weighted between lowest-sell cluster and 30d median)
- `estimate_quicksell(buy_orders) -> ref` (highest genuine buy order, outlier-filtered)
- `trend(history) -> { ma7, ma30, volatility, d1, d7, d30, d365 }`

**TradeAnalysisEngine** — polls trade offers, values both sides via AnalyticsEngine, produces `TradeVerdict { stars: 1..5, net_ref, roi, risk, explanation, counteroffer: Option<...> }`. The explanation is template-composed from the actual factors that drove the score (spread, demand, volatility, overpay ratio) — deterministic and auditable, not hand-wavy.

**FlipFinder** — background scanner over the websocket stream + periodic snapshot sweeps of a candidate universe (watched items, high-volume items, recent movers). Scores each candidate: `expected_profit`, `roi`, `confidence` (history depth × liquidity), `est_sale_time` (from listing-age distribution). Configurable filters, capped scan rate to respect the API.

**AlertService** — rule engine subscribing to `ListingEvent` + daily rollups; dispatches to `NotificationSink` implementations (OS notification, Discord webhook, sound).

**PortfolioService** — daily and on-demand valuation snapshots, P/L windows, winners/losers.

**HistoryRecorder** — writes every observation into `price_points`, maintains `price_daily` rollups.

**Implementation note (Module 8):** `price_points`' columns (`best_buy_ref`+`best_sell_ref`+counts together in one row) are snapshot-shaped, not single-event-shaped — a lone `listing-update` only tells you about one side. So "every observation" is implemented as periodic snapshots (every 15 min) of our own `market_listings` state (itself websocket-fed, per Module 7) — `source='snapshot'` — plus one `source='schema'` observation per item whenever the community price catalog syncs (Module 5's `IGetPrices`), giving immediate-if-shallow history from day one per the original design intent. A separate row-per-raw-event (`source='ws'`) path was scoped out as unnecessary volume for what the schema already captures via periodic snapshots.

**Also discovered live (Module 8):** the websocket's `item` payload carries `killstreakTier`, `australium`, and `festivized` fields when applicable (verified against real Strange/Killstreak/Australium listings) — not just `defindex`/`quality`/`particle` as first captured in Module 5/7. `ListingEvent` now carries these too. Rather than have `HistoryRecorder` maintain a second, duplicate in-memory order book just to get the exact `ItemKey`, Module 7's `market_listings` table itself was extended (migration `0007`) with `killstreak_tier`/`australium`/`festivized`/`craftable` columns — `HistoryRecorder`'s periodic snapshot now aggregates `market_listings` grouped by the *full* key rather than the coarser defindex+quality+effect_id grouping it originally shipped with. Correctness matters more here than in Module 7's classifieds lookups, since mixing a Strange Australium weapon's prices into the base weapon's trend would corrupt the whole point of this module.

**Implementation note (Module 8, schema-sync source):** the community price catalog (`IGetPrices`) has no killstreak/australium/festivized breakdown at all — it's keyed only by defindex+quality(+effect_id for Unusuals), split into `Tradable`/`Non-Tradable` and `Craftable`/`Non-Craftable`. So `source='schema'` observations are necessarily base-permutation only (non-killstreak, non-australium, non-festivized); only the `Tradable` group is used (`ItemKey` has no tradable flag, so blending in `Non-Tradable` prices would misrepresent a tradable item's history). A catalog entry's own `value`/`currency` pair is converted to ref via a key rate derived from the catalog's own Mann Co. Supply Crate Key entry (itself priced in metal) — no separate key-rate source exists yet.

**Implementation note (Module 9):** shipped **pull-based** rather than the background-poller + `trade:analyzed` push event sketched in §7's data-flow diagram — `TradeAnalysisEngine::get_active_trades` recomputes every active received offer's verdict fresh on each call (same "no persistence, recompute on request" shape as Module 7's Market Analyzer), and the frontend polls it on a `TanStack Query` interval. Push events + OS notifications are properly Module 10 (AlertService/NotificationSink)'s job; building that plumbing here would have meant either duplicating it or wiring Module 9 straight into Module 10 out of roadmap order. "Given" (the user's own) items resolve against the already-synced `inventory_items` cache with no extra Steam call; "received" (the trade partner's) items have no local cache, so `IEconItems_440/GetPlayerItems` — the same endpoint Module 3 uses for the user's own inventory — is called against the partner's SteamID64 (derived from the trade offer's `accountid_other`), briefly cached in `kv_cache` since the same pending offer gets re-analyzed every poll. Counteroffers are a numeric suggestion (ref, plus a keys/metal breakdown once a live key rate has been observed via `price_points`) surfaced with a clipboard "copy message" action — not a drag-drop builder (Module 13) and no Steam write-calls (§2: analysis only).

**Implementation note (Module 10):** unlike Module 9, this ships genuinely **push-based** — `MarketDataService`'s broadcast channel (unused since Module 5/7, "no subscriber exists yet") finally gets two subscribers: `services::live_feed` (a pure relay of every `ListingEvent` to the frontend, no DB involved) and `services::alert_service` (the rule engine). Both are spawned from `lib.rs`'s `.setup()` closure rather than `app::build()`, since emitting a Tauri event requires an `AppHandle`, which doesn't exist yet during `app::build()`'s `block_on`. That same spawn timing also surfaced a real bug worth recording: a plain `tokio::spawn` inside `.setup()` panics ("there is no reactor running") because that closure isn't called from inside an entered Tokio runtime the way `app::build()`'s `block_on` is — fixed by using `tauri::async_runtime::spawn` instead, which dispatches onto Tauri's own managed runtime regardless of the caller's context.

`AlertService` covers all six documented rule kinds by mapping each to data already on hand: `price_drop`/`spread_widen`/`new_buyer`/`new_seller` react to individual `ListingEvent`s (spread is only recomputed when a rule actually needs it, to avoid an extra query per event); `hist_low`/`hist_high` are checked by a separate hourly sweep over `price_daily`, per §6's "+ daily rollups" — these have no single triggering event. **Deviation:** the "sound" `NotificationSink` sketched in §4's folder structure (`infra/notify/sound.rs`) doesn't exist server-side — sound plays client-side via Web Audio instead, off the pushed `AlertFired` event's `channels` field. The window is already open and can beep with zero new dependencies or platform audio-device quirks; a Rust audio-decode dependency for this wasn't justified. `desktop` (via the new `tauri-plugin-notification` dependency) and `discord` (a plain webhook POST, URL stored in the OS keychain like the other secrets) are real Rust-side sinks, both best-effort — a failed sink logs a warning rather than aborting the loop.

**Implementation note (Module 11):** shipped **pull-based**, like Module 9 rather than Module 10 — a ranked opportunities list is something the user refreshes/re-filters, not something needing instant push. **Deviation:** §6's "capped scan rate to respect the API" no longer applies post-Module-5 — the "scan" is a pure read over already-ingested `market_listings`/`price_daily`, not per-item external API calls, so there's nothing left to rate-limit. The three named candidate criteria (watched/high-volume/recent-mover) are all derived from *one* valuation pass per active item (`MarketListingsRepo::aggregate_by_item` bounds the universe to what's actually being traded) rather than three separate gather-then-value phases. This module also finally created the `watchlist` table (`docs/DESIGN.md` §5) — specified since Phase 0 but with no consumer until now — and crossed the "rule of three" reuse threshold: Module 7's `market_analyzer_service::analyze_query`, Module 9's `trade_analysis_engine`, and now Flip Finder all independently needed "resolve an `ItemKey` → current listings + `price_daily` history → spread/estimate/liquidity/demand". The Module 9/Flip-Finder shape (single exact `ItemKey`, not Module 7's multi-defindex-by-name resolution) is now shared in `services/item_valuation.rs`, enriched with the history-depth/listing-age/trend fields Flip Finder needs; Trade Analyzer was refactored to call it instead of keeping its own copy.

**Implementation note (Module 12):** the ledger's core problem — a completed trade's items have already changed hands by the time it shows as completed, and "received" items get new asset ids under the new owner, so re-resolving them from scratch generally isn't possible — is solved by reuse rather than invention: Module 9's `get_active_trades` already successfully resolves and values both sides of every active offer on each poll, so it now also best-effort-caches that resolved `{partner_steam_id, given, received, net_ref}` into `kv_cache` (keyed `trade_analysis:{trade_offer_id}`, ~14-day TTL) every time it runs. When `TradeHistoryService::sync_completed_trades` later sees that same offer has completed (Steam's `GetTradeOffers` with `historical_only=1`, filtered to `trade_offer_state == 3`), it promotes the cached analysis straight into the permanent `trades` ledger — no re-resolution needed, and it's exactly the valuation the user saw at the time. A trade with no cached analysis (app wasn't open while it was active) still gets a row rather than silently vanishing from history, with its items labeled unresolved and `net_value_ref: 0.0`. **Deviation:** `given_json`/`received_json` are stored as `{name, value_ref}` rather than §5's sketched `{item_id, asset_id, value_ref}` — the cached analysis they come from only ever carried a display name and estimated value (unresolved items have neither an `item_id` nor `asset_id` to offer), which is sufficient for a historical ledger display that doesn't need FK integrity back to a live item row. `AppState.config` changed from `RwLock<Config>` to `Arc<RwLock<Config>>` so the new daily portfolio-snapshot loop (spawned from `app::build()`, same place as `HistoryRecorder::spawn_periodic`) can share the same config data the managed `AppState` uses — transparent to every existing command since `Arc<RwLock<T>>` auto-derefs to `RwLock<T>`. Portfolio valuation reuses Module 11's `item_valuation::value_item_key`, grouped by distinct `item_id` first so a stack of identical items is only valued once; winners/losers reuses `domain::trend::trend` directly with no new scoring needed.

**Implementation note (Module 13):** the simulator and advanced search are one integrated feature, not two — search exists to power the simulator's "receive" item picker, with no separate standalone search tab. No new domain module was needed: "simulate a trade" is exactly `domain::trade_rating::rate_trade` given manually-assembled sides, and search is a plain DB filter, not a domain computation. The two sides of a hypothetical trade are resolved asymmetrically, matching the realistic mental model: "give" is always the user's own synced inventory, resolved by asset id (`InventoryRepo::find_by_asset_ids`, reusing Module 9's plumbing); "receive" is always a directly-specified hypothetical `ItemKey` (wire-safe as `ItemKeyInput`, `TryFrom` into the domain type), valued the same way Module 9 values a trade partner's items — neither side needs a Steam API call, since nothing is fetched from a live trade offer. `services::simulator_service` deliberately duplicates the small `unresolved()`/`to_valued_item()` helpers from `trade_analysis_engine.rs` rather than extracting a shared abstraction, since this is only the second caller ("rule of three" — see Module 11's `item_valuation` extraction, which happened on the third). `ItemsRepo::search` filters the existing `items` table on name substring / quality / killstreak tier / australium / craftable / has-unusual-effect, requires at least one filter set (an empty query returns nothing rather than the whole catalog), and caps results at 50 rows. Search returns catalog data only, with no live pricing per row — valuing every result live would mean up to 50 sequential DB round-trips per keystroke of a search-as-you-type box; valuation instead happens once, when an item is actually added to a bucket and `simulate_trade` runs. The frontend drag-and-drop uses native HTML5 `draggable`/`ondragstart`/`ondrop` (no new dependency), and every draggable row also gets a click-to-add fallback for accessibility.

**Implementation note (Module 14):** built as a real sandboxed WASM host (`wasmtime`/`wasmtime-wasi`), not a stub — verified end to end in the build environment before writing any production code (a real Rust crate compiled to `wasm32-wasip1`, instantiated with a capability-less WASI shim plus custom host imports, both directions of data crossing, and fuel-based trapping of an infinite loop, all exercised standalone before `infra::plugins` existed). Two ABI conventions, deliberately hand-rolled rather than adopting the full WASM Component Model (unjustified extra machinery for a first plugin consumer): *guest calls host* (`host_*` imports, `infra::plugins::host_functions`) pass an `out_ptr_ptr` cell the host writes a guest-`alloc`'d buffer's pointer into, returning the byte length (or a negative code: `-1` capability denied, `-2` rate limited, `-3` internal); *host calls guest* (`plugin_init`/`on_alert_fired`/`provide_listings`, via `PluginRuntime::call_export_json`) all share the fixed signature `(in_ptr: i32, in_len: i32) -> i64`, the guest packing `(ptr << 32) | len` (`0` = nothing) on return. WASI is wired in only as a capability-less shim (`WasiCtxBuilder::new()`'s defaults — closed stdin, discarded stdout/stderr, no env/args/preopens) purely so `wasm32-wasip1`'s libstd links; every real host capability is gated in `HostContext.granted: Vec<Capability>`, checked before any of the five `host_*` imports (`log`, `market_price_daily`, `inventory_list`, `http_fetch`, `notify_send`) do anything. `Config::consume_fuel(true)` plus a fixed per-call budget makes "sandboxed" a real, tested claim (`an_infinite_loop_export_traps_on_fuel_exhaustion`) rather than marketing. **Deviation:** `notify:send` reaches only the Discord webhook sink (`infra::notify::discord::send`, needs just `reqwest::Client` + a keychain URL) — not OS desktop notifications, which need a live `AppHandle` that `plugin_service`'s otherwise DB-only, fully-unit-testable functions don't have (matching the existing testable-without-a-live-app pattern `alert_service.rs` already established). **MarketDataProvider** (§8's "register new market data provider sources") is delivered without a new trait hierarchy: a plugin declaring the `market_provider` event gets its `provide_listings` export polled periodically, and the resulting `ListingEvent`s are fed into `MarketDataService::inject_external_listings` — the exact same broadcast bus the real backpack.tf websocket feeds (its per-event handling was factored into a shared `handle_event` for this), so Live Feed/History Recorder/Flip Finder/Alerts pick up plugin-sourced listings with no changes at all. **Panels** are static-only (a plugin's `panel/index.html`, served through Tauri's asset protocol into a sandboxed `<iframe sandbox="allow-scripts">` with no `allow-same-origin`) — no plugin↔host `postMessage` bridge in v1, a second RPC surface not yet justified by a real consumer. Capability approval is all-or-nothing at install (the frontend shows the manifest's requested list before confirming); revoking means disabling or uninstalling the whole plugin, not a per-capability toggle. The SDK (`plugin-sdk/`) and the working example (`sample-plugin/`, capabilities `market:read`+`notify:send`, subscribed to both `alert_fired` and `market_provider`, plus a demo panel) are standalone crates at the repo root — not Cargo workspace members of `src-tauri`, since they target `wasm32-wasip1` and CI's backend job only ever runs from inside `src-tauri`; the sample plugin is built once and its `.wasm` output committed as a test fixture (`src-tauri/tests/fixtures/sample_plugin/`), so `plugin_service`'s integration tests exercise a real compiled plugin without requiring the wasm toolchain in CI.

**Implementation note (Module 15):** the last module finally builds the real Dockview docking §9 originally specced — Modules 1–14 shipped a plain `useState`-driven tab bar in `App.tsx` instead, with no docking dependency ever added, so "Workspaces" was still fully open scope going in. `src/app/workspace/WorkspaceShell.tsx` hosts a `dockview-react` grid; the three named example layouts from §9 ("Trading"/"Portfolio"/"Sniping") are built only from panels that actually exist today (no separate "item detail" panel — Module 7 folded that into Market Analyzer). Workspace layouts and the new theme presets are both **frontend-only**, persisted via `zustand/middleware`'s `persist` to `localStorage` — the first use of that middleware in the codebase — rather than round-tripping through `infra::config::Config`/IPC: both are pure display/layout preferences with no domain meaning, and Tauri's webview `localStorage` is already durable local storage, so adding backend config fields and commands for them would be pure overhead. **Deviation:** default layouts are built by adding each panel as a `direction: "right"` split of the previous one (a left-to-right column grid) rather than hand-authoring `SerializedDockview` JSON blobs — far more robust to library internals, and the user can drag panels into tab groups themselves afterward if they prefer that layout; the result auto-saves into that workspace's slot via a debounced `onDidLayoutChange`.

Themes reuse Tailwind's `@theme` token mechanism already established in `src/themes/theme.css`: `--color-fg`/`-muted`/`-subtle` semantic text tokens were added alongside the existing `--color-charcoal*` background tokens, and every hardcoded `text-zinc-*` class across `src/features/**` (14 files, ~100 occurrences) was swept to the new tokens — a plain background-color swap would have shipped an unreadable light theme, since Tailwind's built-in gray-scale utilities don't react to `data-theme` at all. `--color-quality-*` (the TF2 rarity colors) stay fixed across all three presets, per the existing rationale that they're the game's own stable conventions, not brand colors.

Exports are, by contrast, entirely **backend**, matching §3's "frontend never talks to the network [or does other I/O]" rule: `infra::export::{csv,xlsx,pdf}` are pure byte-generating writers (no I/O — the same role `infra::backpack_tf::models` plays for JSON mapping, hence living in `infra/` and not `domain/`, which is TF2 business logic specifically) over a generic `ExportTable { headers, rows }`; `services::export_service` gathers each dataset via the *same* repo/service calls its existing IPC command already uses, maps to `ExportTable` (or, for JSON, serializes the original structured data directly — flattening to strings would throw away type information a JSON consumer would want back), and does the one real `std::fs::write` to a path the frontend obtained via `tauri-plugin-dialog`'s native save picker (a new dependency; no prior "let the user pick a save location" flow existed anywhere). PDF export hand-rolls a simple paginated table over `printpdf`'s `Op`-list API using a built-in Helvetica base-14 font, rather than adopting `genpdf` — `genpdf`'s table layout is more ergonomic but needs a TTF font file bundled into the binary, and PDF's built-in fonts need no font asset at all, which matters more for a packaged desktop app than saving a few dozen lines of manual column layout. `AppError` gained an `Export(String)` variant, matching the existing granularity of Config/Database/Keychain/Network/InvalidInput/Internal.

Packaging stayed local-build-only per this module's agreed scope (see `docs/RELEASE.md`) — `.github/workflows/ci.yml` is unchanged, it still only lints/tests/builds, it does not bundle or publish installers. A real `npm run tauri build` was exercised end to end: `.deb` bundles cleanly (icons and `tauri.conf.json`'s bundle config were already in place since Module 1, contrary to an initial assumption that they were missing); AppImage bundling downloads its toolchain fine but its final step needs the host's `/usr/bin/xdg-open` (from the `xdg-utils` package), which isn't guaranteed present — documented as a prerequisite in `docs/RELEASE.md` rather than worked around, since faking that binary risks producing a silently-broken AppImage.

---

## 7. Data Flow (example: incoming trade)

```
Steam IEconService poll ──► TradeOfferProvider
        │ new offer detected
        ▼
TradeAnalysisEngine ──► InventoryService (identify given assets)
        │           └─► MarketDataService (fresh snapshot per SKU)
        │           └─► HistoryRecorder (30d stats per SKU)
        ▼
AnalyticsEngine (pure): value both sides, spread, demand, risk
        ▼
TradeVerdict { ★★★★, +18.2 keys, explanation }
        ▼
Tauri event "trade:analyzed" ──► React trade panel + OS notification
```

---

## 8. Plugin System

- **Runtime:** WASM via `wasmtime` — sandboxed, cross-platform, language-agnostic (Rust/AssemblyScript/TinyGo plugins).
- **Manifest:** `plugin.toml` declaring name, version, and **requested capabilities** (`market:read`, `inventory:read`, `notify:send`, `http:{allowlisted domains}`). User approves capabilities on install, like a browser extension.
- **Host API (v1 surface):** read item/listing/history data, register alert channels, register new `MarketDataProvider` sources (this is how Marketplace.tf / Mannco.store / Steam Market arrive later without touching core), contribute panels (plugin ships an HTML bundle rendered in a sandboxed panel).
- **Isolation:** plugins never get raw DB or keychain access; all calls go through the capability-checked host API with per-plugin rate limits.

---

## 9. UI / Workspace Design

**Shell:** dark theme default (charcoal `#17181c`, TF2 quality colors as the accent system — Unusual purple `#8650AC`, Strange orange `#CF6A32`, Unique yellow `#FFD700`, Genuine green `#4D7455`). Dockview layout with saveable named workspaces ("Trading", "Portfolio", "Sniping").

```
┌──────────────────────────────────────────────────────────────┐
│ ⌘K Search…        [Trading ▾]  🔔 3   key=67.5 ref   ● live │
├───────────────┬──────────────────────────┬───────────────────┤
│ BACKPACK      │ ITEM: Scorching Team     │ LIVE FEED         │
│ [grid 10×n]   │ Captain                  │ ▲ new sell 405k   │
│ ▩▩▩▩▩▩▩▩▩▩    │ ┌────────┐ Buy: 380k(12)│ ▼ delist 410k     │
│ ▩▩▩▩▩▩▩▩▩▩    │ │  img   │ Sell:405k(3) │ ● buyer +2k       │
│ filter/tags   │ └────────┘ Spread: 6.2%  │───────────────────│
│               │ [chart 30D ────────────] │ FLIP FINDER       │
│ Σ 214 items   │ liq 71 · demand 84       │ 1. +4.1k ROI 9%   │
│ 486.2 keys    │ est sale 398k · QS 380k  │ 2. +1.3k ROI 22%  │
├───────────────┴──────────────────────────┴───────────────────┤
│ INCOMING TRADE  ★★★★  +18.2 keys  [Explain] [Counteroffer]   │
└──────────────────────────────────────────────────────────────┘
```

Backpack grid: virtualized (react-window) so 3000-item backpacks scroll at 60 fps; quality-colored borders, effect sparkle badge, paint dot, killstreak chevrons, hover tooltip with full detail, right-click context menu per spec, ctrl-click multi-select for bulk actions. **Deviation (found live, Module 15):** the hover tooltip rendered *behind* neighboring tiles — react-window positions every cell `absolute` with no `z-index`, so a cell's own `z-20` tooltip only wins within that cell's local stack, not against sibling cells that happen to mount later in DOM order. Fixed with a `hover:z-50` on each cell's own react-window-positioned wrapper, elevating the whole hovered cell (tooltip included) above every sibling.

**Addition (Module 15, requested live):** clicking a backpack item (plain click, not ctrl/cmd-click — that's reserved for bulk-select) now also opens/focuses the Market Analyzer panel pre-populated with that item's live buy/sell orders, by synthesizing a classifieds URL client-side (`features/backpack/api.ts::buildClassifiedUrl`, param names matching `domain/classified_url.rs`'s parser exactly) and feeding it through the *existing* `analyze_classified_url` command — no new backend command needed. A small module-level registry (`app/workspace/dockviewApi.ts`) exposes the Dockview API outside `WorkspaceShell` so a deeply-nested component (an item tile) can open/focus a panel without prop-drilling; `useMarketAnalyzerStore`'s consume-once `pendingUrl` hands the synthesized URL to `MarketAnalyzer.tsx`, which auto-runs the analysis exactly like a pasted URL would.

Scoped deliberately short of the fuller "list on backpack.tf" / "quicksell" request that prompted this: backpack.tf's v2 API does support creating a classifieds listing given the user's token, but **actually sending a Steam trade offer does not work with just a Web API key** — Steam's official `IEconService` only exposes read/manage-your-own-offers endpoints; creating a new offer requires `steamcommunity.com/tradeoffer/new/send`, authenticated with real browser session cookies (`sessionid` + `steamLoginSecure`), not the API key model this app deliberately uses instead (§2: "we never see credentials"). That's also explicitly unofficial/unsupported by Valve. Building real trade-sending would mean capturing and holding actual Steam session credentials — a materially different and riskier trust model than anything else in this app — so it's intentionally not built without a separate decision to cross that line.

Keyboard: `⌘K` universal search, `1–9` workspace switch, `F` favorite, `W` watch, `A` analyze.

---

## 10. Implementation Roadmap (module-by-module, approval-gated)

| # | Module | Delivers | Depends on |
|---|---|---|---|
| 1 | **Foundation** | Tauri scaffold, DI wiring, config, logging, sqlx migrations, keychain, typed IPC codegen, CI | — |
| 2 | **Item Domain + Steam Schema** | SKU model, currency math, schema sync, item DB | 1 |
| 3 | **Steam Auth + Inventory** | OpenID login, inventory sync/cache/diffing | 2 |
| 4 | **Backpack UI** | Virtualized grid, tooltips, tags/folders/favorites, stats bar | 3 |
| 5 | **Backpack.tf Client** | v2 REST + websocket + snapshot, rate limiter, ListingEvent bus | 2 |
| 6 | **Analytics Engine** | All pure scoring/estimation functions + full unit-test suite | 2 |
| 7 | **Item Analytics Panel + Market Analyzer** | Detail panel, classified-URL analyzer, buyers/sellers tables | 4,5,6 |
| 8 | **History Recorder + Charts** | price_points/price_daily pipeline, Lightweight Charts panel | 5,6 |
| 9 | **Trade Analyzer** | Offer polling, valuation, ★ rating, explanations, counteroffers | 3,6,8 |
| 10 | **Live Feed + Alerts** | Feed panel, rule engine, OS/Discord/sound sinks | 5 |
| 11 | **Flip Finder** | Scanner, scoring, ranked opportunities panel | 5,6,8 |
| 12 | **Portfolio + Trade History** | Snapshots, P/L, ledger, performance charts | 3,8,9 |
| 13 | **Simulator + Advanced Search** | Drag-drop trade builder, faceted search | 6,7 |
| 14 | **Plugin System** | WASM host, capability model, SDK, sample plugin | stable API |
| 15 | **Power User + Polish** | Workspaces, exports (CSV/XLSX/JSON/PDF), themes, packaging | all |

Each module ships with: code + rustdoc/TSDoc, unit tests (domain layer targets >90% coverage), error handling via `thiserror` + typed IPC errors, `tracing` instrumentation, and config keys documented in `docs/config.md`.

---

## 11. Cross-Cutting Standards

- **Errors:** no `unwrap()` outside tests; every IPC command returns `Result<T, AppError>` serialized with a stable error code the UI can localize.
- **Rate limiting:** single global `governor` limiter per external host; websocket preferred over polling wherever it exists.
- **Secrets:** Steam API key + backpack.tf token in OS keychain only; never in SQLite, config files, or logs. **Deviation (Module 1, found post-hoc during Module 15's packaging verification):** `infra::keychain` originally used the `keyring` 4.x crate's `v1` convenience API, which on Linux lazily wires up Secret Service (D-Bus) on first use — but if no Secret Service provider is reachable (verified live: this is the default on a plain WSL2 install, no `gnome-keyring`/`kwalletd` running), `v1` marks itself "already initialized" regardless and permanently returns `NoDefaultStore` for the rest of the process, with no way to retry through its own API. `infra::keychain` now goes straight to `keyring-core` and, on Linux, falls back to the kernel keyring (`linux-keyutils-keyring-store`, no daemon required) when Secret Service isn't reachable — logged as a warning, since that fallback only persists `UntilReboot` rather than indefinitely. Still a strict improvement over the keychain being unusable for the whole session, which is what silently happened before.
- **Logging:** `tracing` with JSON file rotation, log level in settings, sensitive fields redacted at the macro level.
- **ADRs:** every architectural decision recorded in `docs/adr/NNN-*.md`, starting with the decisions in this document.

---

## 12. Notes on the AI Market Assistant (Feature 6)

The buy/hold/sell/wait/quicksell recommendations in v1 are **rule-based over the analytics engine** (trend + liquidity + spread + demand thresholds), which makes them explainable and free. An LLM-backed layer is deliberately deferred to a plugin so the core never depends on an external AI API. Every recommendation carries its inputs: "SELL — price is 14% above 90-day median, sell depth thin (3 listings), demand score falling (84→71 over 14d)."
