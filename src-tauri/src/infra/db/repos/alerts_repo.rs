use sqlx::SqlitePool;

use crate::error::AppResult;

pub struct CreateAlertRule<'a> {
    pub item_id: i64,
    pub kind: &'a str,
    pub threshold: Option<f64>,
    /// Pre-serialized JSON array (e.g. `["desktop","sound"]`) — (de)serializing
    /// `channels` is a service-layer concern (`docs/DESIGN.md` §5's `channels`
    /// column comment), this repo just stores/returns the raw text, same as
    /// `inventory_items.raw_json` elsewhere.
    pub channels_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct AlertRuleRow {
    pub id: i32,
    pub item_id: i64,
    pub kind: String,
    pub threshold: Option<f64>,
    pub channels: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct AlertRuleWithItemRow {
    pub id: i32,
    pub item_id: i64,
    pub item_name: String,
    pub kind: String,
    pub threshold: Option<f64>,
    pub channels: String,
    pub enabled: bool,
}

pub struct AlertRulesRepo;

impl AlertRulesRepo {
    pub async fn create(pool: &SqlitePool, rule: &CreateAlertRule<'_>) -> AppResult<i32> {
        let id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO alert_rules (item_id, kind, threshold, channels, enabled)
            VALUES (?, ?, ?, ?, 1)
            RETURNING id
            "#,
        )
        .bind(rule.item_id)
        .bind(rule.kind)
        .bind(rule.threshold)
        .bind(rule.channels_json)
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    /// Every rule, newest first, joined with its item's display name — for
    /// the Alerts panel's rule list.
    pub async fn list(pool: &SqlitePool) -> AppResult<Vec<AlertRuleWithItemRow>> {
        let rows = sqlx::query_as::<_, AlertRuleWithItemRow>(
            r#"
            SELECT ar.id, ar.item_id, it.name AS item_name, ar.kind, ar.threshold, ar.channels, ar.enabled
            FROM alert_rules ar
            JOIN items it ON it.id = ar.item_id
            ORDER BY ar.id DESC
            "#,
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Every enabled rule targeting `item_id` — the lookup `AlertService`
    /// does per incoming `ListingEvent`.
    pub async fn list_enabled_for_item(
        pool: &SqlitePool,
        item_id: i64,
    ) -> AppResult<Vec<AlertRuleRow>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, item_id, kind, threshold, channels, enabled \
             FROM alert_rules WHERE item_id = ? AND enabled = 1",
        )
        .bind(item_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Every enabled rule of one kind, across all items — used by the
    /// periodic hist_low/hist_high sweep, which has no single triggering
    /// `ListingEvent` to key off of (it reacts to `price_daily` rollups).
    pub async fn list_enabled_by_kind(
        pool: &SqlitePool,
        kind: &str,
    ) -> AppResult<Vec<AlertRuleRow>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, item_id, kind, threshold, channels, enabled \
             FROM alert_rules WHERE kind = ? AND enabled = 1",
        )
        .bind(kind)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn set_enabled(pool: &SqlitePool, rule_id: i32, enabled: bool) -> AppResult<()> {
        sqlx::query("UPDATE alert_rules SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(rule_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, rule_id: i32) -> AppResult<()> {
        sqlx::query("DELETE FROM alert_rules WHERE id = ?")
            .bind(rule_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

pub struct InsertAlertEvent<'a> {
    pub rule_id: i32,
    pub fired_ts: i64,
    pub payload_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct AlertEventRow {
    pub id: i32,
    pub rule_id: i32,
    pub fired_ts: i64,
    pub payload: String,
    pub acked: bool,
}

pub struct AlertEventsRepo;

impl AlertEventsRepo {
    pub async fn insert(pool: &SqlitePool, event: &InsertAlertEvent<'_>) -> AppResult<i32> {
        let id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO alert_events (rule_id, fired_ts, payload, acked)
            VALUES (?, ?, ?, 0)
            RETURNING id
            "#,
        )
        .bind(event.rule_id)
        .bind(event.fired_ts)
        .bind(event.payload_json)
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    pub async fn list_recent(pool: &SqlitePool, limit: i64) -> AppResult<Vec<AlertEventRow>> {
        let rows = sqlx::query_as::<_, AlertEventRow>(
            "SELECT id, rule_id, fired_ts, payload, acked FROM alert_events ORDER BY fired_ts DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn ack(pool: &SqlitePool, event_id: i32) -> AppResult<()> {
        sqlx::query("UPDATE alert_events SET acked = 1 WHERE id = ?")
            .bind(event_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "alerts_repo_tests.rs"]
mod tests;
