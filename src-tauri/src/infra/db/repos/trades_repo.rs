use sqlx::SqlitePool;

use crate::error::AppResult;

pub struct InsertTrade<'a> {
    pub trade_offer_id: &'a str,
    pub partner_steam_id: &'a str,
    pub completed_ts: i64,
    pub given_json: &'a str,
    pub received_json: &'a str,
    pub net_value_ref: f64,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct TradeRow {
    pub trade_offer_id: String,
    pub partner_steam_id: String,
    pub completed_ts: i64,
    pub given_json: String,
    pub received_json: String,
    pub net_value_ref: f64,
    pub rating: Option<i64>,
    pub notes: Option<String>,
}

pub struct TradesRepo;

impl TradesRepo {
    /// Idempotent — `trade_offer_id` is the primary key, so re-running a
    /// completed-trade sync over an overlapping historical window never
    /// duplicates a trade. Returns whether a new row was actually
    /// inserted (`false` if it already existed).
    pub async fn insert_if_new(pool: &SqlitePool, trade: &InsertTrade<'_>) -> AppResult<bool> {
        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO trades
                (trade_offer_id, partner_steam_id, completed_ts, given_json, received_json, net_value_ref)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(trade.trade_offer_id)
        .bind(trade.partner_steam_id)
        .bind(trade.completed_ts)
        .bind(trade.given_json)
        .bind(trade.received_json)
        .bind(trade.net_value_ref)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_recent(pool: &SqlitePool, limit: i64) -> AppResult<Vec<TradeRow>> {
        let rows = sqlx::query_as::<_, TradeRow>(
            "SELECT * FROM trades ORDER BY completed_ts DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn set_rating(
        pool: &SqlitePool,
        trade_offer_id: &str,
        rating: Option<i64>,
    ) -> AppResult<()> {
        sqlx::query("UPDATE trades SET rating = ? WHERE trade_offer_id = ?")
            .bind(rating)
            .bind(trade_offer_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn set_notes(
        pool: &SqlitePool,
        trade_offer_id: &str,
        notes: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query("UPDATE trades SET notes = ? WHERE trade_offer_id = ?")
            .bind(notes)
            .bind(trade_offer_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "trades_repo_tests.rs"]
mod tests;
