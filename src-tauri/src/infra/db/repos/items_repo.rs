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
    pub image_url: Option<String>,
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

/// Faceted search filters — the same facets `domain::classified_url`
/// already models from backpack.tf's own URL convention (Module 13).
/// `is_empty` (all `None`) means "no filter specified"; `search` treats
/// that as "return nothing" rather than dumping the whole catalog.
#[derive(Debug, Clone, Copy, Default)]
pub struct ItemSearchFilters<'a> {
    pub name: Option<&'a str>,
    pub quality: Option<u8>,
    pub killstreak_tier: Option<u8>,
    pub australium: Option<bool>,
    pub craftable: Option<bool>,
    pub has_effect: Option<bool>,
}

impl ItemSearchFilters<'_> {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.quality.is_none()
            && self.killstreak_tier.is_none()
            && self.australium.is_none()
            && self.craftable.is_none()
            && self.has_effect.is_none()
    }
}

/// Bounds a search-as-you-type box's result set — search returns catalog
/// data only (no live pricing), so this stays cheap even for a broad
/// query; a specific item gets valued once, when it's actually added to a
/// simulator bucket (Module 13).
const SEARCH_RESULT_LIMIT: i64 = 50;

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

    /// Sets an item's schema image URL — a separate call rather than a
    /// `get_or_create` parameter (Module 15) because `get_or_create` has
    /// ~20 call sites across the codebase, almost none of which have image
    /// data on hand; only `schema_service::sync` does.
    pub async fn set_image_url(pool: &SqlitePool, id: i64, image_url: &str) -> AppResult<()> {
        sqlx::query("UPDATE items SET image_url = ? WHERE id = ?")
            .bind(image_url)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Repairs rows still carrying `inventory_service::sync`'s "Unknown
    /// Item {defindex}" fallback name after a schema sync (Module 15,
    /// found live). Schema sync only ever creates/updates each defindex's
    /// *base*-quality row (the schema has no per-permutation data); a
    /// Strange/Unusual/etc. permutation seeded first by inventory sync is
    /// a *different* row (quality is part of the uniqueness key) that
    /// schema sync never touches, and stays stuck with its fallback name
    /// forever otherwise — `find_name_by_defindex` would resolve it
    /// correctly on a fresh sync, but inventory sync's own diff logic
    /// never re-resolves a name for an item whose raw Steam data hasn't
    /// changed. Copies name + image_url from any same-defindex row that
    /// already has a real name. Returns the number of rows fixed.
    pub async fn backfill_unknown_names(pool: &SqlitePool) -> AppResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE items
            SET name = (
                    SELECT src.name FROM items src
                    WHERE src.defindex = items.defindex AND src.name NOT LIKE 'Unknown Item %'
                    LIMIT 1
                ),
                image_url = (
                    SELECT src.image_url FROM items src
                    WHERE src.defindex = items.defindex AND src.name NOT LIKE 'Unknown Item %'
                    LIMIT 1
                )
            WHERE name LIKE 'Unknown Item %'
              AND EXISTS (
                    SELECT 1 FROM items src
                    WHERE src.defindex = items.defindex AND src.name NOT LIKE 'Unknown Item %'
                )
            "#,
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
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

    /// Faceted catalog search (Module 13) — powers the Simulator's item
    /// picker. Returns nothing (rather than the whole catalog) if
    /// `filters` is entirely unset; otherwise every set facet is AND'd
    /// together, capped at `SEARCH_RESULT_LIMIT`.
    pub async fn search(
        pool: &SqlitePool,
        filters: &ItemSearchFilters<'_>,
    ) -> AppResult<Vec<ItemRow>> {
        if filters.is_empty() {
            return Ok(Vec::new());
        }

        let mut clauses: Vec<&str> = Vec::new();
        if filters.name.is_some() {
            clauses.push("name LIKE ?");
        }
        if filters.quality.is_some() {
            clauses.push("quality = ?");
        }
        if filters.killstreak_tier.is_some() {
            clauses.push("killstreak_tier = ?");
        }
        if filters.australium.is_some() {
            clauses.push("australium = ?");
        }
        if filters.craftable.is_some() {
            clauses.push("craftable = ?");
        }
        if let Some(has_effect) = filters.has_effect {
            clauses.push(if has_effect {
                "effect_id IS NOT NULL"
            } else {
                "effect_id IS NULL"
            });
        }

        // Safe: the only dynamic parts are the clause list (fixed literals
        // chosen from the match arms above, never interpolated user data)
        // and the constant LIMIT — actual filter values are always bound
        // below, never spliced into the SQL text.
        let sql = format!(
            "SELECT * FROM items WHERE {} ORDER BY name LIMIT {SEARCH_RESULT_LIMIT}",
            clauses.join(" AND ")
        );
        let mut query = sqlx::query_as::<_, ItemRow>(sqlx::AssertSqlSafe(sql));

        if let Some(name) = filters.name {
            // No escaping of the user's own `%`/`_` wildcards — a minor,
            // acceptable rough edge for a search box, not a correctness or
            // security concern (the value is still a bound parameter).
            query = query.bind(format!("%{name}%"));
        }
        if let Some(quality) = filters.quality {
            query = query.bind(quality as i64);
        }
        if let Some(killstreak_tier) = filters.killstreak_tier {
            query = query.bind(killstreak_tier as i64);
        }
        if let Some(australium) = filters.australium {
            query = query.bind(australium);
        }
        if let Some(craftable) = filters.craftable {
            query = query.bind(craftable);
        }

        let rows = query.fetch_all(pool).await?;
        Ok(rows)
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
    async fn set_image_url_persists_and_is_readable_back() {
        let (pool, dir) = test_pool().await;

        let key = plain_key(5021);
        let id = ItemsRepo::get_or_create(&pool, &key, "Mann Co. Supply Crate Key")
            .await
            .unwrap();

        let row = ItemsRepo::find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(row.image_url, None);

        ItemsRepo::set_image_url(&pool, id, "https://example.com/key.png")
            .await
            .unwrap();

        let row = ItemsRepo::find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(
            row.image_url,
            Some("https://example.com/key.png".to_string())
        );

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn backfill_unknown_names_copies_name_and_image_from_a_resolved_permutation() {
        let (pool, dir) = test_pool().await;

        // Base-quality row, seeded (and later fixed) by schema sync.
        let base = ItemsRepo::get_or_create(&pool, &plain_key(45), "Unknown Item 45")
            .await
            .unwrap();
        ItemsRepo::set_image_url(&pool, base, "https://example.com/scattergun.png")
            .await
            .unwrap();
        sqlx::query("UPDATE items SET name = ? WHERE id = ?")
            .bind("Scattergun")
            .bind(base)
            .execute(&pool)
            .await
            .unwrap();

        // Strange permutation, seeded by inventory sync before the schema
        // ever ran, and never revisited since.
        let strange_id = ItemsRepo::get_or_create(
            &pool,
            &ItemKey {
                quality: Quality::Strange,
                ..plain_key(45)
            },
            "Unknown Item 45",
        )
        .await
        .unwrap();

        let fixed = ItemsRepo::backfill_unknown_names(&pool).await.unwrap();
        assert_eq!(fixed, 1);

        let row = ItemsRepo::find_by_id(&pool, strange_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.name, "Scattergun");
        assert_eq!(
            row.image_url,
            Some("https://example.com/scattergun.png".to_string())
        );

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn backfill_unknown_names_leaves_genuinely_unresolvable_items_alone() {
        let (pool, dir) = test_pool().await;

        let id = ItemsRepo::get_or_create(&pool, &plain_key(999_999), "Unknown Item 999999")
            .await
            .unwrap();

        let fixed = ItemsRepo::backfill_unknown_names(&pool).await.unwrap();
        assert_eq!(fixed, 0);

        let row = ItemsRepo::find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(row.name, "Unknown Item 999999");

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

    #[tokio::test]
    async fn search_with_no_filters_returns_nothing() {
        let (pool, dir) = test_pool().await;
        ItemsRepo::get_or_create(&pool, &plain_key(45), "Scattergun")
            .await
            .unwrap();

        let results = ItemsRepo::search(&pool, &ItemSearchFilters::default())
            .await
            .unwrap();
        assert!(results.is_empty());

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn search_by_name_substring_is_case_insensitive() {
        let (pool, dir) = test_pool().await;
        ItemsRepo::get_or_create(&pool, &plain_key(45), "Scattergun")
            .await
            .unwrap();
        ItemsRepo::get_or_create(&pool, &plain_key(46), "Pistol")
            .await
            .unwrap();

        let results = ItemsRepo::search(
            &pool,
            &ItemSearchFilters {
                name: Some("scatter"),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Scattergun");

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn search_by_quality_filters_exactly() {
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

        let results = ItemsRepo::search(
            &pool,
            &ItemSearchFilters {
                quality: Some(Quality::Strange as u8),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].quality, Quality::Strange as u8 as i64);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn search_by_has_effect_distinguishes_unusuals() {
        let (pool, dir) = test_pool().await;
        ItemsRepo::get_or_create(&pool, &plain_key(45), "Scattergun")
            .await
            .unwrap();
        ItemsRepo::get_or_create(
            &pool,
            &ItemKey {
                quality: Quality::Unusual,
                effect_id: Some(13),
                ..plain_key(30469)
            },
            "Team Captain",
        )
        .await
        .unwrap();

        let unusuals = ItemsRepo::search(
            &pool,
            &ItemSearchFilters {
                has_effect: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(unusuals.len(), 1);
        assert_eq!(unusuals[0].name, "Team Captain");

        let non_unusuals = ItemsRepo::search(
            &pool,
            &ItemSearchFilters {
                has_effect: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(non_unusuals.len(), 1);
        assert_eq!(non_unusuals[0].name, "Scattergun");

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn search_combines_multiple_filters_with_and() {
        let (pool, dir) = test_pool().await;
        ItemsRepo::get_or_create(&pool, &plain_key(45), "Scattergun")
            .await
            .unwrap();
        ItemsRepo::get_or_create(
            &pool,
            &ItemKey {
                australium: true,
                ..plain_key(45)
            },
            "Scattergun",
        )
        .await
        .unwrap();

        let results = ItemsRepo::search(
            &pool,
            &ItemSearchFilters {
                name: Some("Scattergun"),
                australium: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].australium);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn search_respects_the_result_limit() {
        let (pool, dir) = test_pool().await;
        for defindex in 0..60u32 {
            ItemsRepo::get_or_create(&pool, &plain_key(defindex), "Duplicate Name")
                .await
                .unwrap();
        }

        let results = ItemsRepo::search(
            &pool,
            &ItemSearchFilters {
                name: Some("Duplicate"),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(results.len(), SEARCH_RESULT_LIMIT as usize);

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }
}
