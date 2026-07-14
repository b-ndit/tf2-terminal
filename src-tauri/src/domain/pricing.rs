//! Spread and price-estimation functions over a market's active listings.
//! Pure — no I/O, no clock reads; all inputs are explicit.
//!
//! Delivered in Module 6 per the roadmap but not called from application
//! code until Module 7 (Item Analytics Panel + Market Analyzer) and Module
//! 9 (Trade Analyzer) wire it up — fully exercised by unit tests until
//! then.
#![allow(dead_code)]

use crate::domain::trend::PricePoint;

const DAY_SECONDS: i64 = 86_400;
const THIRTY_DAYS_SECONDS: i64 = 30 * DAY_SECONDS;
/// How many of the cheapest sell listings form the "lowest-sell cluster"
/// in [`estimate_sale_price`].
const SELL_CLUSTER_SIZE: usize = 3;
/// Buy orders priced more than this many median-absolute-deviations above
/// the median are treated as outliers (likely fake/troll listings) and
/// excluded from [`estimate_quicksell`].
const QUICKSELL_OUTLIER_MAD_MULTIPLIER: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listing {
    pub intent: Intent,
    pub price_ref: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    pub abs_ref: f64,
    pub pct: f64,
}

/// The gap between the best (highest) buy offer and the best (lowest) sell
/// ask. `None` if either side has no listings.
pub fn spread(listings: &[Listing]) -> Option<Spread> {
    let best_buy = listings
        .iter()
        .filter(|l| l.intent == Intent::Buy)
        .map(|l| l.price_ref)
        .fold(None, max_option);
    let best_sell = listings
        .iter()
        .filter(|l| l.intent == Intent::Sell)
        .map(|l| l.price_ref)
        .fold(None, min_option);

    match (best_buy, best_sell) {
        (Some(buy), Some(sell)) if sell > 0.0 => {
            let abs_ref = sell - buy;
            Some(Spread {
                abs_ref,
                pct: abs_ref / sell * 100.0,
            })
        }
        _ => None,
    }
}

/// Blends the "lowest-sell cluster" (what you're realistically competing
/// against right now) with the 30-day median (a longer-run anchor), 60/40.
/// Falls back to whichever side has data if only one does.
pub fn estimate_sale_price(
    listings: &[Listing],
    history: &[PricePoint],
    now_ts: i64,
) -> Option<f64> {
    let mut sell_prices: Vec<f64> = listings
        .iter()
        .filter(|l| l.intent == Intent::Sell)
        .map(|l| l.price_ref)
        .collect();
    sell_prices.sort_by(|a, b| a.partial_cmp(b).expect("prices are never NaN"));
    let cluster_avg = if sell_prices.is_empty() {
        None
    } else {
        let take = sell_prices.len().min(SELL_CLUSTER_SIZE);
        Some(mean(&sell_prices[..take]))
    };

    let window_start = now_ts - THIRTY_DAYS_SECONDS;
    let mut recent: Vec<f64> = history
        .iter()
        .filter(|p| p.ts >= window_start && p.ts <= now_ts)
        .map(|p| p.value_ref)
        .collect();
    let median_30d = median(&mut recent);

    match (cluster_avg, median_30d) {
        (Some(c), Some(m)) => Some(0.6 * c + 0.4 * m),
        (Some(c), None) => Some(c),
        (None, Some(m)) => Some(m),
        (None, None) => None,
    }
}

/// The highest buy order, after excluding outliers that are implausibly
/// far above the median (likely fake/troll listings rather than genuine
/// buying interest).
pub fn estimate_quicksell(buy_orders: &[Listing]) -> Option<f64> {
    let prices: Vec<f64> = buy_orders
        .iter()
        .filter(|l| l.intent == Intent::Buy)
        .map(|l| l.price_ref)
        .collect();
    if prices.is_empty() {
        return None;
    }
    let med = median(&mut prices.clone())?;
    let mad = median_absolute_deviation(&prices, med);
    // A zero MAD (e.g. all prices identical) would make the threshold
    // collapse to the median itself; give it a floor proportional to the
    // median so identical-priced listings aren't spuriously excluded.
    let threshold = med + QUICKSELL_OUTLIER_MAD_MULTIPLIER * mad.max(med * 0.01);

    prices
        .into_iter()
        .filter(|p| *p <= threshold)
        .fold(None, max_option)
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
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

fn median_absolute_deviation(values: &[f64], median_value: f64) -> f64 {
    let mut deviations: Vec<f64> = values.iter().map(|v| (v - median_value).abs()).collect();
    median(&mut deviations).unwrap_or(0.0)
}

fn max_option(acc: Option<f64>, value: f64) -> Option<f64> {
    Some(acc.map_or(value, |a| a.max(value)))
}

fn min_option(acc: Option<f64>, value: f64) -> Option<f64> {
    Some(acc.map_or(value, |a| a.min(value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buy(price_ref: f64) -> Listing {
        Listing {
            intent: Intent::Buy,
            price_ref,
        }
    }

    fn sell(price_ref: f64) -> Listing {
        Listing {
            intent: Intent::Sell,
            price_ref,
        }
    }

    #[test]
    fn spread_uses_best_buy_and_best_sell() {
        let listings = vec![buy(50.0), buy(45.0), sell(60.0), sell(65.0)];
        let s = spread(&listings).unwrap();
        assert!((s.abs_ref - 10.0).abs() < 1e-9);
        assert!((s.pct - (10.0 / 60.0 * 100.0)).abs() < 1e-9);
    }

    #[test]
    fn spread_is_none_without_both_sides() {
        assert_eq!(spread(&[buy(50.0)]), None);
        assert_eq!(spread(&[sell(50.0)]), None);
        assert_eq!(spread(&[]), None);
    }

    #[test]
    fn estimate_sale_price_blends_cluster_and_median() {
        let now = 1_000_000_000;
        let listings = vec![sell(90.0), sell(95.0), sell(100.0), sell(200.0)];
        let history = vec![
            PricePoint {
                ts: now,
                value_ref: 110.0,
            },
            PricePoint {
                ts: now - DAY_SECONDS,
                value_ref: 110.0,
            },
        ];
        // cluster = mean(90,95,100) = 95; median_30d = 110
        let estimate = estimate_sale_price(&listings, &history, now).unwrap();
        assert!((estimate - (0.6 * 95.0 + 0.4 * 110.0)).abs() < 1e-9);
    }

    #[test]
    fn estimate_sale_price_falls_back_to_cluster_without_history() {
        let listings = vec![sell(100.0)];
        let estimate = estimate_sale_price(&listings, &[], 1_000_000_000).unwrap();
        assert!((estimate - 100.0).abs() < 1e-9);
    }

    #[test]
    fn estimate_sale_price_falls_back_to_median_without_sell_listings() {
        let now = 1_000_000_000;
        let history = vec![PricePoint {
            ts: now,
            value_ref: 42.0,
        }];
        let estimate = estimate_sale_price(&[], &history, now).unwrap();
        assert!((estimate - 42.0).abs() < 1e-9);
    }

    #[test]
    fn estimate_sale_price_is_none_with_no_data_at_all() {
        assert_eq!(estimate_sale_price(&[], &[], 1_000_000_000), None);
    }

    #[test]
    fn estimate_quicksell_takes_highest_genuine_buy_order() {
        let orders = vec![buy(10.0), buy(12.0), buy(11.0)];
        assert_eq!(estimate_quicksell(&orders), Some(12.0));
    }

    #[test]
    fn estimate_quicksell_filters_out_implausible_high_outlier() {
        // A cluster of genuine offers around 10-12, plus one troll listing
        // at 10,000 — the outlier should not set the quicksell price.
        let orders = vec![buy(10.0), buy(11.0), buy(12.0), buy(10.0), buy(10_000.0)];
        assert_eq!(estimate_quicksell(&orders), Some(12.0));
    }

    #[test]
    fn estimate_quicksell_ignores_sell_listings() {
        let orders = vec![sell(100.0)];
        assert_eq!(estimate_quicksell(&orders), None);
    }

    #[test]
    fn estimate_quicksell_is_none_for_empty_input() {
        assert_eq!(estimate_quicksell(&[]), None);
    }

    #[test]
    fn estimate_quicksell_handles_identical_prices() {
        let orders = vec![buy(5.0), buy(5.0), buy(5.0)];
        assert_eq!(estimate_quicksell(&orders), Some(5.0));
    }
}
