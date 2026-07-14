pub mod repos;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
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
        .foreign_keys(true);

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
