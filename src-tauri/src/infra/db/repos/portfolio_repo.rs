use sqlx::SqlitePool;

use crate::error::AppResult;

pub struct InsertPortfolioSnapshot<'a> {
    pub ts: i64,
    pub steam_id: &'a str,
    pub total_ref: f64,
    pub total_keys: f64,
    pub pure_keys: Option<i64>,
    pub pure_metal_ref: Option<f64>,
    pub item_count: Option<i64>,
    pub unusual_count: Option<i64>,
    pub australium_count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct PortfolioSnapshotRow {
    pub ts: i64,
    pub steam_id: String,
    pub total_ref: f64,
    pub total_keys: f64,
    pub pure_keys: Option<i64>,
    pub pure_metal_ref: Option<f64>,
    pub item_count: Option<i64>,
    pub unusual_count: Option<i64>,
    pub australium_count: Option<i64>,
}

pub struct PortfolioSnapshotsRepo;

impl PortfolioSnapshotsRepo {
    /// Upserted on `ts` (the primary key) — a re-run "on-demand" snapshot
    /// within the same second as another simply replaces it rather than
    /// erroring.
    pub async fn insert(pool: &SqlitePool, snap: &InsertPortfolioSnapshot<'_>) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO portfolio_snapshots
                (ts, steam_id, total_ref, total_keys, pure_keys, pure_metal_ref,
                 item_count, unusual_count, australium_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(ts) DO UPDATE SET
                steam_id = excluded.steam_id,
                total_ref = excluded.total_ref,
                total_keys = excluded.total_keys,
                pure_keys = excluded.pure_keys,
                pure_metal_ref = excluded.pure_metal_ref,
                item_count = excluded.item_count,
                unusual_count = excluded.unusual_count,
                australium_count = excluded.australium_count
            "#,
        )
        .bind(snap.ts)
        .bind(snap.steam_id)
        .bind(snap.total_ref)
        .bind(snap.total_keys)
        .bind(snap.pure_keys)
        .bind(snap.pure_metal_ref)
        .bind(snap.item_count)
        .bind(snap.unusual_count)
        .bind(snap.australium_count)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn latest(
        pool: &SqlitePool,
        steam_id: &str,
    ) -> AppResult<Option<PortfolioSnapshotRow>> {
        let row = sqlx::query_as::<_, PortfolioSnapshotRow>(
            "SELECT * FROM portfolio_snapshots WHERE steam_id = ? ORDER BY ts DESC LIMIT 1",
        )
        .bind(steam_id)
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }

    pub async fn history_since(
        pool: &SqlitePool,
        steam_id: &str,
        since_ts: i64,
    ) -> AppResult<Vec<PortfolioSnapshotRow>> {
        let rows = sqlx::query_as::<_, PortfolioSnapshotRow>(
            "SELECT * FROM portfolio_snapshots WHERE steam_id = ? AND ts >= ? ORDER BY ts",
        )
        .bind(steam_id)
        .bind(since_ts)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// The most recent snapshot at or before `ts` — "what was the
    /// portfolio worth N days ago" for P/L windows (Module 12). As-of
    /// lookup since snapshots are sparse/not necessarily daily-aligned,
    /// same semantics as `domain::trend`'s history lookups.
    pub async fn most_recent_before(
        pool: &SqlitePool,
        steam_id: &str,
        ts: i64,
    ) -> AppResult<Option<PortfolioSnapshotRow>> {
        let row = sqlx::query_as::<_, PortfolioSnapshotRow>(
            "SELECT * FROM portfolio_snapshots WHERE steam_id = ? AND ts <= ? ORDER BY ts DESC LIMIT 1",
        )
        .bind(steam_id)
        .bind(ts)
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
#[path = "portfolio_repo_tests.rs"]
mod tests;
