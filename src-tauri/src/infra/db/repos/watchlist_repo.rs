use sqlx::SqlitePool;

use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct WatchlistItemRow {
    pub item_id: i64,
    pub item_name: String,
    pub added_ts: i64,
}

pub struct WatchlistRepo;

impl WatchlistRepo {
    /// Idempotent: watching an already-watched item is a no-op, not an
    /// error (`UNIQUE(item_id)`, migration `0009`).
    pub async fn add(pool: &SqlitePool, item_id: i64, added_ts: i64) -> AppResult<()> {
        sqlx::query("INSERT OR IGNORE INTO watchlist (item_id, added_ts) VALUES (?, ?)")
            .bind(item_id)
            .bind(added_ts)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn remove(pool: &SqlitePool, item_id: i64) -> AppResult<()> {
        sqlx::query("DELETE FROM watchlist WHERE item_id = ?")
            .bind(item_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Every watched item's id, unenriched — the lookup `FlipFinder`'s
    /// scan does to tag candidates as `is_watched` (Module 11).
    pub async fn list_item_ids(pool: &SqlitePool) -> AppResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT item_id FROM watchlist")
            .fetch_all(pool)
            .await?;
        Ok(ids)
    }

    /// Joined with the item's display name, newest first — for the
    /// watchlist panel.
    pub async fn list_with_items(pool: &SqlitePool) -> AppResult<Vec<WatchlistItemRow>> {
        let rows = sqlx::query_as::<_, WatchlistItemRow>(
            r#"
            SELECT w.item_id, it.name AS item_name, w.added_ts
            FROM watchlist w
            JOIN items it ON it.id = w.item_id
            ORDER BY w.added_ts DESC
            "#,
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "watchlist_repo_tests.rs"]
mod tests;
