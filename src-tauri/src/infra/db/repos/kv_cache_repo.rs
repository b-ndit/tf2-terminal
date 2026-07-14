use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;

use crate::error::AppResult;

/// Generic TTL'd key-value cache (schema blobs, image metadata, etc. — see
/// `docs/DESIGN.md` §5's `kv_cache` table).
pub struct KvCacheRepo;

impl KvCacheRepo {
    /// Returns `None` if the key is missing or has expired.
    pub async fn get(pool: &SqlitePool, key: &str) -> AppResult<Option<Vec<u8>>> {
        let row: Option<(Vec<u8>, Option<i64>)> =
            sqlx::query_as("SELECT value, expires_ts FROM kv_cache WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;

        let Some((value, expires_ts)) = row else {
            return Ok(None);
        };

        if let Some(expires_ts) = expires_ts {
            if expires_ts < now_unix() {
                return Ok(None);
            }
        }

        Ok(Some(value))
    }

    pub async fn set(pool: &SqlitePool, key: &str, value: &[u8], ttl: Duration) -> AppResult<()> {
        let expires_ts = now_unix() + ttl.as_secs() as i64;
        sqlx::query(
            r#"
            INSERT INTO kv_cache (key, value, expires_ts) VALUES (?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, expires_ts = excluded.expires_ts
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(expires_ts)
        .execute(pool)
        .await?;
        Ok(())
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::db;

    async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tf2-terminal-kv-cache-test-{}-{}",
            std::process::id(),
            uniq_suffix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");
        let pool = db::init_pool(&db_path).await.unwrap();
        (pool, dir)
    }

    fn uniq_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    #[tokio::test]
    async fn set_then_get_round_trips() {
        let (pool, dir) = test_pool().await;

        KvCacheRepo::set(&pool, "k", b"hello", Duration::from_secs(60))
            .await
            .unwrap();
        let value = KvCacheRepo::get(&pool, "k").await.unwrap();

        assert_eq!(value, Some(b"hello".to_vec()));

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn get_returns_none_for_missing_key() {
        let (pool, dir) = test_pool().await;

        assert_eq!(KvCacheRepo::get(&pool, "missing").await.unwrap(), None);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn get_returns_none_for_expired_entry() {
        let (pool, dir) = test_pool().await;

        // TTL of 0 means expires_ts == now, so it reads as already expired
        // on the very next check.
        KvCacheRepo::set(&pool, "k", b"stale", Duration::from_secs(0))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;

        assert_eq!(KvCacheRepo::get(&pool, "k").await.unwrap(), None);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn set_overwrites_existing_key() {
        let (pool, dir) = test_pool().await;

        KvCacheRepo::set(&pool, "k", b"first", Duration::from_secs(60))
            .await
            .unwrap();
        KvCacheRepo::set(&pool, "k", b"second", Duration::from_secs(60))
            .await
            .unwrap();

        assert_eq!(
            KvCacheRepo::get(&pool, "k").await.unwrap(),
            Some(b"second".to_vec())
        );

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }
}
