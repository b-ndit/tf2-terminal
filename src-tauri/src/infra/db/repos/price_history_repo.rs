//! `price_points`/`price_daily` repo. Delivered in Module 8 per the roadmap
//! but not called until `services::history_recorder` lands later in this
//! same module — fully exercised by unit tests until then.
#![allow(dead_code)]

use sqlx::SqlitePool;

use crate::error::AppResult;

pub struct InsertPricePoint<'a> {
    pub item_id: i64,
    pub ts: i64,
    pub source: &'a str,
    pub best_buy_keys: Option<f64>,
    pub best_buy_ref: Option<f64>,
    pub best_sell_keys: Option<f64>,
    pub best_sell_ref: Option<f64>,
    pub buy_count: Option<i64>,
    pub sell_count: Option<i64>,
    pub key_rate_ref: f64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PricePointRow {
    pub ts: i64,
    pub best_buy_ref: Option<f64>,
    pub best_sell_ref: Option<f64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PriceDailyRow {
    pub day: i64,
    pub open_ref: Option<f64>,
    pub high_ref: Option<f64>,
    pub low_ref: Option<f64>,
    pub close_ref: Option<f64>,
    pub avg_ref: Option<f64>,
    pub median_ref: Option<f64>,
    pub samples: Option<i64>,
}

/// The midpoint of best buy/sell, or whichever side is present — the
/// single representative "price" a raw `price_points` row rolls up into
/// for OHLC purposes.
pub(crate) fn mid_value(buy: Option<f64>, sell: Option<f64>) -> Option<f64> {
    match (buy, sell) {
        (Some(b), Some(s)) => Some((b + s) / 2.0),
        (Some(b), None) => Some(b),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    }
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("prices are never NaN"));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

pub struct PricePointsRepo;

impl PricePointsRepo {
    pub async fn insert(pool: &SqlitePool, point: &InsertPricePoint<'_>) -> AppResult<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO price_points
                (item_id, ts, source, best_buy_keys, best_buy_ref, best_sell_keys, best_sell_ref,
                 buy_count, sell_count, key_rate_ref)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(point.item_id)
        .bind(point.ts)
        .bind(point.source)
        .bind(point.best_buy_keys)
        .bind(point.best_buy_ref)
        .bind(point.best_sell_keys)
        .bind(point.best_sell_ref)
        .bind(point.buy_count)
        .bind(point.sell_count)
        .bind(point.key_rate_ref)
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    pub async fn history_since(
        pool: &SqlitePool,
        item_id: i64,
        since_ts: i64,
    ) -> AppResult<Vec<PricePointRow>> {
        let rows = sqlx::query_as::<_, PricePointRow>(
            "SELECT ts, best_buy_ref, best_sell_ref FROM price_points \
             WHERE item_id = ? AND ts >= ? ORDER BY ts",
        )
        .bind(item_id)
        .bind(since_ts)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// The key↔ref rate captured on the most recent `price_points` row,
    /// across all items — every row already carries a `key_rate_ref` at
    /// capture time (`docs/DESIGN.md` §5), so the latest one is a live
    /// rate without re-deriving it from the price catalog. `None` before
    /// any observation has ever been recorded.
    pub async fn latest_key_rate(pool: &SqlitePool) -> AppResult<Option<f64>> {
        let rate: Option<f64> =
            sqlx::query_scalar("SELECT key_rate_ref FROM price_points ORDER BY ts DESC LIMIT 1")
                .fetch_optional(pool)
                .await?;
        Ok(rate)
    }
}

pub struct PriceDailyRepo;

impl PriceDailyRepo {
    /// Recomputes and upserts `day`'s rollup from whatever `price_points`
    /// rows exist for `item_id` that day. No-ops if there are none (e.g.
    /// called for a day before this item had any observations).
    pub async fn recompute_day(pool: &SqlitePool, item_id: i64, day: i64) -> AppResult<()> {
        const DAY_SECONDS: i64 = 86_400;
        let day_start = day * DAY_SECONDS;
        let day_end = day_start + DAY_SECONDS;

        let rows: Vec<PricePointRow> = sqlx::query_as(
            "SELECT ts, best_buy_ref, best_sell_ref FROM price_points \
             WHERE item_id = ? AND ts >= ? AND ts < ? ORDER BY ts",
        )
        .bind(item_id)
        .bind(day_start)
        .bind(day_end)
        .fetch_all(pool)
        .await?;

        let mut values: Vec<f64> = rows
            .iter()
            .filter_map(|r| mid_value(r.best_buy_ref, r.best_sell_ref))
            .collect();
        if values.is_empty() {
            return Ok(());
        }

        let open = values[0];
        let close = *values.last().expect("non-empty");
        let high = values.iter().cloned().fold(f64::MIN, f64::max);
        let low = values.iter().cloned().fold(f64::MAX, f64::min);
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        let samples = values.len() as i64;
        let median_value = median(&mut values);

        sqlx::query(
            r#"
            INSERT INTO price_daily
                (item_id, day, open_ref, high_ref, low_ref, close_ref, avg_ref, median_ref, samples)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(item_id, day) DO UPDATE SET
                open_ref = excluded.open_ref,
                high_ref = excluded.high_ref,
                low_ref = excluded.low_ref,
                close_ref = excluded.close_ref,
                avg_ref = excluded.avg_ref,
                median_ref = excluded.median_ref,
                samples = excluded.samples
            "#,
        )
        .bind(item_id)
        .bind(day)
        .bind(open)
        .bind(high)
        .bind(low)
        .bind(close)
        .bind(avg)
        .bind(median_value)
        .bind(samples)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn history_since(
        pool: &SqlitePool,
        item_id: i64,
        since_day: i64,
    ) -> AppResult<Vec<PriceDailyRow>> {
        let rows = sqlx::query_as::<_, PriceDailyRow>(
            "SELECT day, open_ref, high_ref, low_ref, close_ref, avg_ref, median_ref, samples \
             FROM price_daily WHERE item_id = ? AND day >= ? ORDER BY day",
        )
        .bind(item_id)
        .bind(since_day)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "price_history_repo_tests.rs"]
mod tests;
