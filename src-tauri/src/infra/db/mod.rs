pub mod repos;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::AppResult;

/// Opens the SQLite pool in WAL mode (creating the DB file if missing) and
/// runs pending migrations from `infra/db/migrations`.
pub async fn init_pool(db_path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        // sqlx/SQLite default `synchronous=FULL` fsyncs on *every* commit,
        // even in WAL mode — with `market_listings` upserted roughly
        // once per incoming websocket event (the live feed is a genuine
        // firehose across the whole TF2 economy), that meant an fsync per
        // listing. Verified live: "slow statement" warnings over 1s for a
        // single indexed upsert on an otherwise-tiny table, with schema
        // sync's own writes queuing up behind them. `NORMAL` is SQLite's
        // own documented pairing for WAL mode — the WAL itself already
        // makes a crash safe against corruption, so the only real
        // tradeoff is losing the last few not-yet-checkpointed writes on
        // an OS-level power loss (not an app crash) — fine for cached
        // market data, not acceptable for something like a ledger.
        .synchronous(SqliteSynchronous::Normal);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    sqlx::migrate!("src/infra/db/migrations").run(&pool).await?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_pool_creates_db_and_runs_migrations() {
        let dir = std::env::temp_dir().join(format!("tf2-terminal-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");

        let pool = init_pool(&db_path).await.unwrap();

        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM kv_cache")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 0);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }
}
