use sqlx::SqlitePool;

use crate::domain::item::ItemKey;
use crate::error::AppResult;

pub struct UpsertMarketListing<'a> {
    pub listing_id: &'a str,
    pub defindex: i64,
    pub quality: i64,
    pub effect_id: Option<i64>,
    pub killstreak_tier: i64,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
    pub intent: &'a str,
    pub price_ref: f64,
    pub steam_id: &'a str,
    pub steam_name: Option<&'a str>,
    pub updated_at: i64,
}

/// One exact item permutation's current buy/sell depth, aggregated live
/// from `market_listings` — the shape `HistoryRecorder` snapshots into
/// `price_points` (`docs/DESIGN.md` §6, Module 8).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ItemAggregateRow {
    pub defindex: i64,
    pub quality: i64,
    pub effect_id: Option<i64>,
    pub killstreak_tier: i64,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
    pub best_buy_ref: Option<f64>,
    pub best_sell_ref: Option<f64>,
    pub buy_count: i64,
    pub sell_count: i64,
}

/// Pure DB projection — see the note on `InventoryItemView` in
/// `inventory_repo.rs`: SQLite's native `i64` isn't exposed directly via
/// IPC (Specta forbids it); `services::market_analyzer_service` converts
/// this into a TS-safe DTO.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MarketListingRow {
    pub listing_id: String,
    pub defindex: i64,
    // Every row in a result set already matches the quality/effect_id the
    // caller filtered by, so these are redundant per-row in current
    // production code — kept for row completeness and query-correctness
    // tests (see market_listings_repo_tests.rs).
    #[allow(dead_code)]
    pub quality: i64,
    #[allow(dead_code)]
    pub effect_id: Option<i64>,
    pub intent: String,
    pub price_ref: f64,
    pub steam_id: String,
    pub steam_name: Option<String>,
    pub updated_at: i64,
}

pub struct MarketListingsRepo;

impl MarketListingsRepo {
    pub async fn upsert(pool: &SqlitePool, row: &UpsertMarketListing<'_>) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO market_listings
                (listing_id, defindex, quality, effect_id, killstreak_tier, australium, festivized,
                 craftable, intent, price_ref, steam_id, steam_name, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(listing_id) DO UPDATE SET
                defindex = excluded.defindex,
                quality = excluded.quality,
                effect_id = excluded.effect_id,
                killstreak_tier = excluded.killstreak_tier,
                australium = excluded.australium,
                festivized = excluded.festivized,
                craftable = excluded.craftable,
                intent = excluded.intent,
                price_ref = excluded.price_ref,
                steam_id = excluded.steam_id,
                steam_name = excluded.steam_name,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(row.listing_id)
        .bind(row.defindex)
        .bind(row.quality)
        .bind(row.effect_id)
        .bind(row.killstreak_tier)
        .bind(row.australium)
        .bind(row.festivized)
        .bind(row.craftable)
        .bind(row.intent)
        .bind(row.price_ref)
        .bind(row.steam_id)
        .bind(row.steam_name)
        .bind(row.updated_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, listing_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM market_listings WHERE listing_id = ?")
            .bind(listing_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// All currently-cached listings for any of `defindexes` at `quality`
    /// (optionally narrowed to one unusual `effect_id`) — several
    /// defindexes can share one display name (class-specific reskins),
    /// so callers resolve a name to a list first.
    pub async fn list_for_defindexes(
        pool: &SqlitePool,
        defindexes: &[i64],
        quality: i64,
        effect_id: Option<i64>,
    ) -> AppResult<Vec<MarketListingRow>> {
        if defindexes.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; defindexes.len()].join(",");
        let effect_clause = if effect_id.is_some() {
            "AND effect_id = ?"
        } else {
            ""
        };
        let sql = format!(
            "SELECT * FROM market_listings WHERE defindex IN ({placeholders}) AND quality = ? {effect_clause}"
        );

        // Safe: the only dynamic parts are the placeholder count (from
        // defindexes.len()) and a fixed literal clause, not interpolated
        // data.
        let mut query = sqlx::query_as::<_, MarketListingRow>(sqlx::AssertSqlSafe(sql));
        for defindex in defindexes {
            query = query.bind(defindex);
        }
        query = query.bind(quality);
        if let Some(effect_id) = effect_id {
            query = query.bind(effect_id);
        }

        let rows = query.fetch_all(pool).await?;
        Ok(rows)
    }

    /// All currently-cached listings (both sides) for one *exact* item
    /// permutation — unlike `list_for_defindexes` (quality + optional
    /// effect_id only), this matches all seven `ItemKey` columns, needed
    /// by Module 9's per-trade-item valuation where killstreak/australium/
    /// festivized/craftable genuinely change the price. `effect_id IS ?`
    /// (not `=`) so a `None` key correctly matches NULL rows — SQLite's
    /// `IS` is NULL-safe equality, unlike `=`.
    pub async fn list_for_item_key(
        pool: &SqlitePool,
        key: &ItemKey,
    ) -> AppResult<Vec<MarketListingRow>> {
        let rows = sqlx::query_as::<_, MarketListingRow>(
            "SELECT * FROM market_listings \
             WHERE defindex = ? AND quality = ? AND effect_id IS ? AND killstreak_tier = ? \
               AND australium = ? AND festivized = ? AND craftable = ?",
        )
        .bind(key.defindex as i64)
        .bind(key.quality as u8 as i64)
        .bind(key.effect_id.map(|e| e as i64))
        .bind(key.killstreak_tier as u8 as i64)
        .bind(key.australium)
        .bind(key.festivized)
        .bind(key.craftable)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// One row per exact item permutation currently represented in the live
    /// listings cache, with its current best buy/sell depth — the input
    /// `HistoryRecorder` snapshots into `price_points` (Module 8).
    pub async fn aggregate_by_item(pool: &SqlitePool) -> AppResult<Vec<ItemAggregateRow>> {
        let rows = sqlx::query_as::<_, ItemAggregateRow>(
            r#"
            SELECT
                defindex, quality, effect_id, killstreak_tier, australium, festivized, craftable,
                MAX(CASE WHEN intent = 'buy' THEN price_ref END) AS best_buy_ref,
                MIN(CASE WHEN intent = 'sell' THEN price_ref END) AS best_sell_ref,
                COUNT(CASE WHEN intent = 'buy' THEN 1 END) AS buy_count,
                COUNT(CASE WHEN intent = 'sell' THEN 1 END) AS sell_count
            FROM market_listings
            GROUP BY defindex, quality, effect_id, killstreak_tier, australium, festivized, craftable
            "#,
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "market_listings_repo_tests.rs"]
mod tests;
