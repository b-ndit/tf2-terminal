use sqlx::SqlitePool;

use crate::error::AppResult;

pub struct InsertPlugin<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub entry_file: &'a str,
    pub capabilities_json: &'a str,
    pub events_json: &'a str,
    pub has_panel: bool,
    pub installed_ts: i64,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct PluginRow {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub entry_file: String,
    pub capabilities_json: String,
    pub events_json: String,
    pub has_panel: bool,
    pub enabled: bool,
    pub installed_ts: i64,
}

pub struct PluginsRepo;

impl PluginsRepo {
    pub async fn insert(pool: &SqlitePool, plugin: &InsertPlugin<'_>) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO plugins
                (name, version, entry_file, capabilities_json, events_json,
                 has_panel, enabled, installed_ts)
            VALUES (?, ?, ?, ?, ?, ?, 1, ?)
            "#,
        )
        .bind(plugin.name)
        .bind(plugin.version)
        .bind(plugin.entry_file)
        .bind(plugin.capabilities_json)
        .bind(plugin.events_json)
        .bind(plugin.has_panel)
        .bind(plugin.installed_ts)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list(pool: &SqlitePool) -> AppResult<Vec<PluginRow>> {
        let rows =
            sqlx::query_as::<_, PluginRow>("SELECT * FROM plugins ORDER BY installed_ts DESC")
                .fetch_all(pool)
                .await?;
        Ok(rows)
    }

    pub async fn list_enabled(pool: &SqlitePool) -> AppResult<Vec<PluginRow>> {
        let rows = sqlx::query_as::<_, PluginRow>(
            "SELECT * FROM plugins WHERE enabled = 1 ORDER BY installed_ts DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn find_by_name(pool: &SqlitePool, name: &str) -> AppResult<Option<PluginRow>> {
        let row = sqlx::query_as::<_, PluginRow>("SELECT * FROM plugins WHERE name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await?;
        Ok(row)
    }

    pub async fn set_enabled(pool: &SqlitePool, name: &str, enabled: bool) -> AppResult<()> {
        sqlx::query("UPDATE plugins SET enabled = ? WHERE name = ?")
            .bind(enabled)
            .bind(name)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, name: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM plugins WHERE name = ?")
            .bind(name)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "plugins_repo_tests.rs"]
mod tests;
