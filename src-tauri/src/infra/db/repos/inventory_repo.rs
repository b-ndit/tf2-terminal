use sqlx::SqlitePool;

use crate::error::AppResult;

pub struct UpsertInventoryItem<'a> {
    pub asset_id: &'a str,
    pub item_id: i64,
    pub steam_id: &'a str,
    pub craft_number: Option<i64>,
    pub paint_id: Option<i64>,
    pub strange_count: Option<i64>,
    pub tradable: bool,
    pub marketable: Option<bool>,
    pub last_seen_ts: i64,
    pub raw_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ExistingAsset {
    pub asset_id: String,
    pub item_id: i64,
    pub raw_json: String,
}

/// Pure DB projection — SQLite's native `INTEGER` decodes to `i64`, which is
/// fine internally but Specta forbids exporting `i64` to TypeScript (silent
/// precision loss risk). `services::backpack_service` converts this into a
/// TS-safe DTO before it crosses the IPC boundary.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct InventoryItemView {
    pub asset_id: String,
    pub item_id: i64,
    /// Added in Module 12 — `PortfolioService` needs the full `ItemKey`
    /// (defindex included) to reuse `item_valuation::value_item_key`;
    /// nothing before this needed it off this particular view.
    pub defindex: i64,
    pub name: String,
    pub quality: i64,
    pub effect_id: Option<i64>,
    pub killstreak_tier: i64,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
    pub craft_number: Option<i64>,
    pub paint_id: Option<i64>,
    pub strange_count: Option<i64>,
    pub tradable: bool,
    pub marketable: Option<bool>,
    pub acquired_ts: Option<i64>,
    pub last_seen_ts: i64,
}

pub struct InventoryRepo;

impl InventoryRepo {
    /// Everything currently on record for `steam_id`, for diffing against a
    /// fresh fetch (see `services::inventory_service`).
    pub async fn existing_for_steam_id(
        pool: &SqlitePool,
        steam_id: &str,
    ) -> AppResult<Vec<ExistingAsset>> {
        let rows = sqlx::query_as::<_, ExistingAsset>(
            "SELECT asset_id, item_id, raw_json FROM inventory_items WHERE steam_id = ?",
        )
        .bind(steam_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Inserts or fully updates one asset. `acquired_ts` is set on first
    /// insert and deliberately left out of `DO UPDATE SET` so it's
    /// preserved across syncs (see the diffing note in
    /// `services::inventory_service`).
    pub async fn upsert(pool: &SqlitePool, row: &UpsertInventoryItem<'_>) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO inventory_items
                (asset_id, item_id, steam_id, craft_number, paint_id, strange_count,
                 tradable, marketable, acquired_ts, last_seen_ts, raw_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(asset_id) DO UPDATE SET
                item_id = excluded.item_id,
                craft_number = excluded.craft_number,
                paint_id = excluded.paint_id,
                strange_count = excluded.strange_count,
                tradable = excluded.tradable,
                marketable = excluded.marketable,
                last_seen_ts = excluded.last_seen_ts,
                raw_json = excluded.raw_json
            "#,
        )
        .bind(row.asset_id)
        .bind(row.item_id)
        .bind(row.steam_id)
        .bind(row.craft_number)
        .bind(row.paint_id)
        .bind(row.strange_count)
        .bind(row.tradable)
        .bind(row.marketable)
        .bind(row.last_seen_ts)
        .bind(row.last_seen_ts)
        .bind(row.raw_json)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Cheap freshness bump for an asset whose content hasn't changed since
    /// the last sync — avoids the full upsert's extra columns.
    pub async fn touch_last_seen(pool: &SqlitePool, asset_id: &str, ts: i64) -> AppResult<()> {
        sqlx::query("UPDATE inventory_items SET last_seen_ts = ? WHERE asset_id = ?")
            .bind(ts)
            .bind(asset_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Resolves a handful of asset ids (e.g. a trade offer's `items_to_give`)
    /// against the cached inventory for `steam_id`, without fetching
    /// everything. Reuses [`ExistingAsset`]'s shape rather than adding a
    /// narrower projection — the extra `raw_json` column is cheap for the
    /// small row counts a single trade offer involves.
    pub async fn find_by_asset_ids(
        pool: &SqlitePool,
        steam_id: &str,
        asset_ids: &[String],
    ) -> AppResult<Vec<ExistingAsset>> {
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; asset_ids.len()].join(",");
        let sql = format!(
            "SELECT asset_id, item_id, raw_json FROM inventory_items \
             WHERE steam_id = ? AND asset_id IN ({placeholders})"
        );
        // Safe: the only dynamic part is the placeholder *count*, derived
        // from asset_ids.len(), not any interpolated data (same pattern as
        // remove_by_asset_ids below).
        let mut query = sqlx::query_as::<_, ExistingAsset>(sqlx::AssertSqlSafe(sql)).bind(steam_id);
        for id in asset_ids {
            query = query.bind(id);
        }
        Ok(query.fetch_all(pool).await?)
    }

    /// Removes assets no longer present in the latest fetch (traded away,
    /// consumed, etc).
    pub async fn remove_by_asset_ids(pool: &SqlitePool, asset_ids: &[String]) -> AppResult<()> {
        if asset_ids.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; asset_ids.len()].join(",");
        let sql = format!("DELETE FROM inventory_items WHERE asset_id IN ({placeholders})");
        // Safe: the only dynamic part is the placeholder *count*, derived
        // from asset_ids.len(), not any interpolated data.
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in asset_ids {
            query = query.bind(id);
        }
        query.execute(pool).await?;
        Ok(())
    }

    // Consumed by Module 4's stats bar ("Σ 214 items"); unit-tested here in
    // the meantime.
    #[allow(dead_code)]
    pub async fn count_for_steam_id(pool: &SqlitePool, steam_id: &str) -> AppResult<i64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM inventory_items WHERE steam_id = ?")
                .bind(steam_id)
                .fetch_one(pool)
                .await?;
        Ok(count)
    }

    pub async fn list_with_items(
        pool: &SqlitePool,
        steam_id: &str,
    ) -> AppResult<Vec<InventoryItemView>> {
        let rows = sqlx::query_as::<_, InventoryItemView>(
            r#"
            SELECT
                inv.asset_id, inv.item_id, it.defindex, it.name, it.quality, it.effect_id, it.killstreak_tier,
                it.australium, it.festivized, it.craftable,
                inv.craft_number, inv.paint_id, inv.strange_count, inv.tradable, inv.marketable,
                inv.acquired_ts, inv.last_seen_ts
            FROM inventory_items inv
            JOIN items it ON it.id = inv.item_id
            WHERE inv.steam_id = ?
            ORDER BY inv.last_seen_ts DESC
            "#,
        )
        .bind(steam_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "inventory_repo_tests.rs"]
mod tests;
