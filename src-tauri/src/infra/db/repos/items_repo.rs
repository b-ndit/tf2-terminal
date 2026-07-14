use sqlx::SqlitePool;

use crate::domain::item::{ItemKey, KillstreakTier, Quality};
use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ItemRow {
    pub id: i64,
    pub defindex: i64,
    pub name: String,
    pub quality: i64,
    pub effect_id: Option<i64>,
    pub killstreak_tier: i64,
    pub australium: bool,
    pub festivized: bool,
    pub craftable: bool,
}

impl ItemRow {
    // Consumed by Module 4's item detail panel; unit-tested here in the
    // meantime.
    #[allow(dead_code)]
    pub fn key(&self) -> Result<ItemKey, crate::domain::item::ItemError> {
        Ok(ItemKey {
            defindex: self.defindex as u32,
            quality: Quality::try_from(self.quality as u8)?,
            effect_id: self.effect_id.map(|e| e as u32),
            killstreak_tier: KillstreakTier::try_from(self.killstreak_tier as u8)?,
            australium: self.australium,
            festivized: self.festivized,
            craftable: self.craftable,
        })
    }
}

pub struct ItemsRepo;

impl ItemsRepo {
    /// Inserts the item permutation if it doesn't exist yet, or updates its
    /// display name if it does. Returns the row id either way.
    pub async fn get_or_create(pool: &SqlitePool, key: &ItemKey, name: &str) -> AppResult<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO items
                (defindex, name, quality, effect_id, killstreak_tier, australium, festivized, craftable)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(defindex, quality, effect_id_key, killstreak_tier, australium, festivized, craftable)
            DO UPDATE SET name = excluded.name
            RETURNING id
            "#,
        )
        .bind(key.defindex as i64)
        .bind(name)
        .bind(key.quality as u8 as i64)
        .bind(key.effect_id.map(|e| e as i64))
        .bind(key.killstreak_tier as u8 as i64)
        .bind(key.australium)
        .bind(key.festivized)
        .bind(key.craftable)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }

    /// Looks up an existing item's row id by its exact key, without
    /// creating one — used by `HistoryRecorder` (Module 8), which can only
    /// record price history against items the schema/inventory sync has
    /// already seeded a display name for.
    pub async fn find_id_by_key(pool: &SqlitePool, key: &ItemKey) -> AppResult<Option<i64>> {
        let id: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT id FROM items
            WHERE defindex = ? AND quality = ? AND effect_id_key = ?
              AND killstreak_tier = ? AND australium = ? AND festivized = ? AND craftable = ?
            "#,
        )
        .bind(key.defindex as i64)
        .bind(key.quality as u8 as i64)
        .bind(key.effect_id.map(|e| e as i64).unwrap_or(-1))
        .bind(key.killstreak_tier as u8 as i64)
        .bind(key.australium)
        .bind(key.festivized)
        .bind(key.craftable)
        .fetch_optional(pool)
        .await?;
        Ok(id)
    }

    // Consumed by Module 4/7's item detail lookups; unit-tested here in the
    // meantime.
    #[allow(dead_code)]
    pub async fn find_by_id(pool: &SqlitePool, id: i64) -> AppResult<Option<ItemRow>> {
        let row = sqlx::query_as::<_, ItemRow>("SELECT * FROM items WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        Ok(row)
    }

    pub async fn count(pool: &SqlitePool) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
            .fetch_one(pool)
            .await?;
        Ok(count)
    }

    /// Looks up every defindex sharing an exact display name — the reverse
    /// of `find_name_by_defindex`. Several defindexes can share one name
    /// (class-specific reskins etc.), which is exactly why a classifieds
    /// URL's `item` name resolves to a *list* of defindexes to aggregate
    /// listings across (Module 7).
    pub async fn find_defindexes_by_name(pool: &SqlitePool, name: &str) -> AppResult<Vec<i64>> {
        let defindexes: Vec<i64> =
            sqlx::query_scalar("SELECT DISTINCT defindex FROM items WHERE name = ?")
                .bind(name)
                .fetch_all(pool)
                .await?;
        Ok(defindexes)
    }

    /// Looks up a display name for `defindex` regardless of quality —
    /// GetPlayerItems doesn't give us a name, but the schema sync (Module 2)
    /// already seeded this defindex under its base quality, and the name is
    /// the same across quality permutations.
    pub async fn find_name_by_defindex(
        pool: &SqlitePool,
        defindex: u32,
    ) -> AppResult<Option<String>> {
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM items WHERE defindex = ? LIMIT 1")
                .bind(defindex as i64)
                .fetch_optional(pool)
                .await?;
        Ok(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::db;

    async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tf2-terminal-items-repo-test-{}-{}",
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

    fn plain_key(defindex: u32) -> ItemKey {
        ItemKey {
            defindex,
            quality: Quality::Unique,
            effect_id: None,
            killstreak_tier: KillstreakTier::None,
            australium: false,
            festivized: false,
            craftable: true,
        }
    }

    #[tokio::test]
    async fn get_or_create_inserts_new_item() {
        let (pool, dir) = test_pool().await;

        let key = plain_key(5021);
        let id = ItemsRepo::get_or_create(&pool, &key, "Mann Co. Supply Crate Key")
            .await
            .unwrap();

        let row = ItemsRepo::find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(row.name, "Mann Co. Supply Crate Key");
        assert_eq!(row.key().unwrap(), key);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn get_or_create_is_idempotent_for_non_unusual_items() {
        let (pool, dir) = test_pool().await;

        let key = plain_key(5021);
        let id1 = ItemsRepo::get_or_create(&pool, &key, "Key").await.unwrap();
        let id2 = ItemsRepo::get_or_create(&pool, &key, "Key (renamed)")
            .await
            .unwrap();

        assert_eq!(id1, id2);
        assert_eq!(ItemsRepo::count(&pool).await.unwrap(), 1);
        let row = ItemsRepo::find_by_id(&pool, id1).await.unwrap().unwrap();
        assert_eq!(row.name, "Key (renamed)");

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn get_or_create_distinguishes_by_effect_id() {
        let (pool, dir) = test_pool().await;

        let unusual_a = ItemKey {
            effect_id: Some(13),
            ..ItemKey {
                quality: Quality::Unusual,
                ..plain_key(30469)
            }
        };
        let unusual_b = ItemKey {
            effect_id: Some(18),
            ..unusual_a.clone()
        };

        let id_a = ItemsRepo::get_or_create(&pool, &unusual_a, "Team Captain")
            .await
            .unwrap();
        let id_b = ItemsRepo::get_or_create(&pool, &unusual_b, "Team Captain")
            .await
            .unwrap();

        assert_ne!(id_a, id_b);
        assert_eq!(ItemsRepo::count(&pool).await.unwrap(), 2);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn get_or_create_does_not_duplicate_null_effect_id_rows() {
        let (pool, dir) = test_pool().await;

        // Regression test for the SQLite NULL-in-UNIQUE gotcha: repeated
        // upserts of a non-Unusual item (effect_id NULL) must hit the same
        // row every time, not accumulate duplicates.
        let key = plain_key(30743);
        for _ in 0..5 {
            ItemsRepo::get_or_create(&pool, &key, "Some Weapon")
                .await
                .unwrap();
        }

        assert_eq!(ItemsRepo::count(&pool).await.unwrap(), 1);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn find_name_by_defindex_matches_across_quality_permutations() {
        let (pool, dir) = test_pool().await;

        // Seeded (e.g. by schema sync) under the base Normal-quality entry.
        let base_key = ItemKey {
            quality: Quality::Normal,
            ..plain_key(45)
        };
        ItemsRepo::get_or_create(&pool, &base_key, "Scattergun")
            .await
            .unwrap();

        let name = ItemsRepo::find_name_by_defindex(&pool, 45).await.unwrap();
        assert_eq!(name, Some("Scattergun".to_string()));

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn find_name_by_defindex_returns_none_for_unknown_defindex() {
        let (pool, dir) = test_pool().await;
        let name = ItemsRepo::find_name_by_defindex(&pool, 999_999)
            .await
            .unwrap();
        assert_eq!(name, None);
        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn find_defindexes_by_name_finds_all_matching_permutations() {
        let (pool, dir) = test_pool().await;

        ItemsRepo::get_or_create(&pool, &plain_key(45), "Scattergun")
            .await
            .unwrap();
        ItemsRepo::get_or_create(
            &pool,
            &ItemKey {
                quality: Quality::Strange,
                ..plain_key(45)
            },
            "Scattergun",
        )
        .await
        .unwrap();
        ItemsRepo::get_or_create(&pool, &plain_key(46), "Pistol")
            .await
            .unwrap();

        let defindexes = ItemsRepo::find_defindexes_by_name(&pool, "Scattergun")
            .await
            .unwrap();
        assert_eq!(defindexes, vec![45]);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn find_id_by_key_matches_exact_permutation_only() {
        let (pool, dir) = test_pool().await;

        let base = plain_key(45);
        let id = ItemsRepo::get_or_create(&pool, &base, "Scattergun")
            .await
            .unwrap();

        let found = ItemsRepo::find_id_by_key(&pool, &base).await.unwrap();
        assert_eq!(found, Some(id));

        let strange_variant = ItemKey {
            quality: Quality::Strange,
            ..base
        };
        assert_eq!(
            ItemsRepo::find_id_by_key(&pool, &strange_variant)
                .await
                .unwrap(),
            None
        );

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn find_defindexes_by_name_returns_empty_for_unknown_name() {
        let (pool, dir) = test_pool().await;
        let defindexes = ItemsRepo::find_defindexes_by_name(&pool, "Nonexistent Item")
            .await
            .unwrap();
        assert!(defindexes.is_empty());
        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }
}
