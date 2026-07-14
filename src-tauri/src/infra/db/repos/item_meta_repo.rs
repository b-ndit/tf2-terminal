use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;

use crate::error::AppResult;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Type, sqlx::FromRow)]
pub struct ItemMeta {
    pub folder: Option<String>,
    pub pinned: bool,
    pub favorite: bool,
    pub note: Option<String>,
    pub custom_label: Option<String>,
}

pub struct ItemMetaRepo;

impl ItemMetaRepo {
    // Consumed by Module 7's item detail panel (single-asset lookup);
    // get_many below is what backpack_service uses today. Unit-tested here
    // in the meantime.
    #[allow(dead_code)]
    pub async fn get(pool: &SqlitePool, asset_id: &str) -> AppResult<Option<ItemMeta>> {
        let row = sqlx::query_as::<_, ItemMeta>(
            "SELECT folder, pinned, favorite, note, custom_label FROM item_meta WHERE asset_id = ?",
        )
        .bind(asset_id)
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }

    pub async fn set_favorite(pool: &SqlitePool, asset_id: &str, favorite: bool) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO item_meta (asset_id, favorite) VALUES (?, ?)
            ON CONFLICT(asset_id) DO UPDATE SET favorite = excluded.favorite
            "#,
        )
        .bind(asset_id)
        .bind(favorite)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_pinned(pool: &SqlitePool, asset_id: &str, pinned: bool) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO item_meta (asset_id, pinned) VALUES (?, ?)
            ON CONFLICT(asset_id) DO UPDATE SET pinned = excluded.pinned
            "#,
        )
        .bind(asset_id)
        .bind(pinned)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_folder(
        pool: &SqlitePool,
        asset_id: &str,
        folder: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO item_meta (asset_id, folder) VALUES (?, ?)
            ON CONFLICT(asset_id) DO UPDATE SET folder = excluded.folder
            "#,
        )
        .bind(asset_id)
        .bind(folder)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_note(pool: &SqlitePool, asset_id: &str, note: Option<&str>) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO item_meta (asset_id, note) VALUES (?, ?)
            ON CONFLICT(asset_id) DO UPDATE SET note = excluded.note
            "#,
        )
        .bind(asset_id)
        .bind(note)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_custom_label(
        pool: &SqlitePool,
        asset_id: &str,
        custom_label: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO item_meta (asset_id, custom_label) VALUES (?, ?)
            ON CONFLICT(asset_id) DO UPDATE SET custom_label = excluded.custom_label
            "#,
        )
        .bind(asset_id)
        .bind(custom_label)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// All item_meta rows for the given asset ids, keyed by asset_id — used
    /// to enrich the inventory view in one query instead of N+1.
    pub async fn get_many(
        pool: &SqlitePool,
        steam_id: &str,
    ) -> AppResult<std::collections::HashMap<String, ItemMeta>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            asset_id: String,
            #[sqlx(flatten)]
            meta: ItemMeta,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT im.asset_id, im.folder, im.pinned, im.favorite, im.note, im.custom_label
            FROM item_meta im
            JOIN inventory_items inv ON inv.asset_id = im.asset_id
            WHERE inv.steam_id = ?
            "#,
        )
        .bind(steam_id)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|r| (r.asset_id, r.meta)).collect())
    }
}

#[cfg(test)]
#[path = "item_meta_repo_tests.rs"]
mod tests;
