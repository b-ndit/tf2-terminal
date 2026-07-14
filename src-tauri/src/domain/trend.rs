//! Moving averages, volatility, and change-window calculations over a
//! sparse price history. Pure — the caller supplies `now_ts` explicitly
//! (rather than this module reading the clock) so results stay
//! deterministic and testable.
//!
//! Delivered in Module 6 per the roadmap but not called from application
//! code until Module 7/8 (Item Analytics Panel, History Recorder) wire it
//! up — fully exercised by unit tests until then.
#![allow(dead_code)]

const DAY_SECONDS: i64 = 86_400;

/// One historical observation — mirrors a row in `price_daily`/
/// `price_points` (`docs/DESIGN.md` §5), but as a plain value with no DB
/// dependency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PricePoint {
    pub ts: i64,
    pub value_ref: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Trend {
    pub ma7: Option<f64>,
    pub ma30: Option<f64>,
    /// Standard deviation of day-over-day percent returns within the last
    /// 30 days — a conventional volatility measure. `None` if there are
    /// fewer than two daily returns to compute from.
    pub volatility: Option<f64>,
    pub d1: Option<f64>,
    pub d7: Option<f64>,
    pub d30: Option<f64>,
    pub d365: Option<f64>,
}

pub fn trend(history: &[PricePoint], now_ts: i64) -> Trend {
    Trend {
        ma7: moving_average(history, now_ts, 7),
        ma30: moving_average(history, now_ts, 30),
        volatility: volatility_30d(history, now_ts),
        d1: pct_change(history, now_ts, 1),
        d7: pct_change(history, now_ts, 7),
        d30: pct_change(history, now_ts, 30),
        d365: pct_change(history, now_ts, 365),
    }
}

/// Mean of all observations within the trailing `days`-day window ending
/// at `now_ts` (inclusive). `None` if the window has no observations.
fn moving_average(history: &[PricePoint], now_ts: i64, days: i64) -> Option<f64> {
    let window_start = now_ts - days * DAY_SECONDS;
    let values: Vec<f64> = history
        .iter()
        .filter(|p| p.ts >= window_start && p.ts <= now_ts)
        .map(|p| p.value_ref)
        .collect();
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

/// The latest observation at or before `ts`. History is sparse and not
/// necessarily daily-aligned, so this is "as-of" lookup, not an exact
/// match.
fn value_at_or_before(history: &[PricePoint], ts: i64) -> Option<f64> {
    history
        .iter()
        .filter(|p| p.ts <= ts)
        .max_by_key(|p| p.ts)
        .map(|p| p.value_ref)
}

/// Percent change from the value as-of `days` ago to the latest value
/// as-of `now_ts`. `None` if either endpoint has no data.
fn pct_change(history: &[PricePoint], now_ts: i64, days: i64) -> Option<f64> {
    let latest = value_at_or_before(history, now_ts)?;
    let past = value_at_or_before(history, now_ts - days * DAY_SECONDS)?;
    if past == 0.0 {
        return None;
    }
    Some((latest - past) / past * 100.0)
}

/// Standard deviation of day-over-day percent returns within the trailing
/// 30 days. Needs at least two points in the window to define a return.
fn volatility_30d(history: &[PricePoint], now_ts: i64) -> Option<f64> {
    let window_start = now_ts - 30 * DAY_SECONDS;
    let mut points: Vec<&PricePoint> = history
        .iter()
        .filter(|p| p.ts >= window_start && p.ts <= now_ts)
        .collect();
    if points.len() < 2 {
        return None;
    }
    points.sort_by_key(|p| p.ts);

    let returns: Vec<f64> = points
        .windows(2)
        .filter_map(|pair| {
            let (prev, curr) = (pair[0], pair[1]);
            if prev.value_ref == 0.0 {
                None
            } else {
                Some((curr.value_ref - prev.value_ref) / prev.value_ref * 100.0)
            }
        })
        .collect();

    if returns.len() < 2 {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    Some(variance.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(days_ago: i64, value_ref: f64, now: i64) -> PricePoint {
        PricePoint {
            ts: now - days_ago * DAY_SECONDS,
            value_ref,
        }
    }

    #[test]
    fn moving_average_averages_points_in_window() {
        let now = 1_000_000_000;
        let history = vec![
            point(1, 10.0, now),
            point(3, 20.0, now),
            point(10, 100.0, now),
        ];
        // 7-day window includes the first two points but not the 10-day-old one.
        let ma7 = moving_average(&history, now, 7).unwrap();
        assert!((ma7 - 15.0).abs() < 1e-9);
    }

    #[test]
    fn moving_average_is_none_for_empty_window() {
        let now = 1_000_000_000;
        let history = vec![point(100, 10.0, now)];
        assert_eq!(moving_average(&history, now, 7), None);
    }

    #[test]
    fn pct_change_computes_relative_change_over_window() {
        let now = 1_000_000_000;
        let history = vec![point(0, 110.0, now), point(1, 100.0, now)];
        let d1 = pct_change(&history, now, 1).unwrap();
        assert!((d1 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn pct_change_uses_as_of_lookup_for_sparse_history() {
        let now = 1_000_000_000;
        // Only a point from 10 days ago exists for the "7 days ago" lookup;
        // as-of semantics should still find it.
        let history = vec![point(0, 120.0, now), point(10, 100.0, now)];
        let d7 = pct_change(&history, now, 7).unwrap();
        assert!((d7 - 20.0).abs() < 1e-9);
    }

    #[test]
    fn pct_change_is_none_without_a_past_reference_point() {
        let now = 1_000_000_000;
        let history = vec![point(0, 120.0, now)];
        assert_eq!(pct_change(&history, now, 7), None);
    }

    #[test]
    fn volatility_is_zero_for_constant_prices() {
        let now = 1_000_000_000;
        let history = vec![
            point(0, 100.0, now),
            point(1, 100.0, now),
            point(2, 100.0, now),
        ];
        let vol = volatility_30d(&history, now).unwrap();
        assert!(vol.abs() < 1e-9);
    }

    #[test]
    fn volatility_is_positive_for_fluctuating_prices() {
        let now = 1_000_000_000;
        let history = vec![
            point(0, 110.0, now),
            point(1, 90.0, now),
            point(2, 110.0, now),
            point(3, 90.0, now),
        ];
        let vol = volatility_30d(&history, now).unwrap();
        assert!(vol > 0.0);
    }

    #[test]
    fn volatility_is_none_with_fewer_than_two_points() {
        let now = 1_000_000_000;
        let history = vec![point(0, 100.0, now)];
        assert_eq!(volatility_30d(&history, now), None);
    }

    #[test]
    fn trend_reports_none_fields_for_empty_history() {
        let t = trend(&[], 1_000_000_000);
        assert_eq!(t, Trend::default());
    }

    #[test]
    fn trend_populates_all_fields_with_enough_history() {
        let now = 1_000_000_000;
        let mut history = Vec::new();
        for days_ago in 0..400 {
            history.push(point(days_ago, 100.0 + days_ago as f64, now));
        }
        let t = trend(&history, now);
        assert!(t.ma7.is_some());
        assert!(t.ma30.is_some());
        assert!(t.volatility.is_some());
        assert!(t.d1.is_some());
        assert!(t.d7.is_some());
        assert!(t.d30.is_some());
        assert!(t.d365.is_some());
    }
}
