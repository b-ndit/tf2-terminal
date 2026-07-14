use std::collections::HashMap;

use serde::Serialize;
use specta::Type;
use sqlx::SqlitePool;

use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, Serialize, Type, sqlx::FromRow)]
pub struct Tag {
    pub id: i32,
    pub name: String,
    pub color: String,
}

pub struct TagsRepo;

impl TagsRepo {
    pub async fn list(pool: &SqlitePool) -> AppResult<Vec<Tag>> {
        let rows = sqlx::query_as::<_, Tag>("SELECT id, name, color FROM tags ORDER BY name")
            .fetch_all(pool)
            .await?;
        Ok(rows)
    }

    /// Creates the tag, or updates its color if the name already exists
    /// (tag names are unique). Returns the tag id either way.
    pub async fn create(pool: &SqlitePool, name: &str, color: &str) -> AppResult<i32> {
        let id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO tags (name, color) VALUES (?, ?)
            ON CONFLICT(name) DO UPDATE SET color = excluded.color
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(color)
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    pub async fn delete(pool: &SqlitePool, tag_id: i32) -> AppResult<()> {
        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(tag_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn add_to_item(pool: &SqlitePool, asset_id: &str, tag_id: i32) -> AppResult<()> {
        sqlx::query("INSERT OR IGNORE INTO item_tags (asset_id, tag_id) VALUES (?, ?)")
            .bind(asset_id)
            .bind(tag_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn remove_from_item(pool: &SqlitePool, asset_id: &str, tag_id: i32) -> AppResult<()> {
        sqlx::query("DELETE FROM item_tags WHERE asset_id = ? AND tag_id = ?")
            .bind(asset_id)
            .bind(tag_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// All tags for every asset belonging to `steam_id`, keyed by asset_id —
    /// used to enrich the inventory view in one query instead of N+1.
    pub async fn get_many_for_steam_id(
        pool: &SqlitePool,
        steam_id: &str,
    ) -> AppResult<HashMap<String, Vec<Tag>>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            asset_id: String,
            #[sqlx(flatten)]
            tag: Tag,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT it.asset_id, t.id, t.name, t.color
            FROM item_tags it
            JOIN tags t ON t.id = it.tag_id
            JOIN inventory_items inv ON inv.asset_id = it.asset_id
            WHERE inv.steam_id = ?
            "#,
        )
        .bind(steam_id)
        .fetch_all(pool)
        .await?;

        let mut by_asset: HashMap<String, Vec<Tag>> = HashMap::new();
        for row in rows {
            by_asset.entry(row.asset_id).or_default().push(row.tag);
        }
        Ok(by_asset)
    }
}

#[cfg(test)]
#[path = "tags_repo_tests.rs"]
mod tests;
