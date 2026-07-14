//! Liquidity and demand scoring. Both are 0..100 heuristics, not
//! statistically "true" values — the design goal is that each is
//! monotonic in its inputs and each component is individually explainable
//! (`docs/DESIGN.md` §12: recommendations must carry their inputs), so a
//! UI can show e.g. "depth 24/40, freshness 18/30, volume 9/30" rather
//! than a single opaque number. Pure — no I/O, no clock reads.
//!
//! Delivered in Module 6 per the roadmap but not called from application
//! code until Module 7 (Item Analytics Panel + Market Analyzer) wires it
//! up — fully exercised by unit tests until then.
#![allow(dead_code)]

const HOURS_PER_WEEK: f64 = 168.0;

/// Diminishing-returns scaling: `sqrt(x)` capped at `sqrt(cap)`, normalized
/// to `0..1`. Used so e.g. going from 1 to 10 listings matters a lot more
/// than going from 91 to 100.
fn diminishing(x: f64, cap: f64) -> f64 {
    (x.max(0.0).sqrt() / cap.sqrt()).min(1.0)
}

/// Liquidity: how easily this item could be bought or sold right now.
/// Weighted from three components (depth 40, freshness 30, recent volume
/// 30):
/// - `depth`: total active listings (both sides). More depth = more
///   liquid, with diminishing returns past ~100 listings.
/// - `listing_ages_hours`: age of each active listing. A market of
///   brand-new listings is more liquid than one of stale week-old ones.
/// - `volume_7d`: completed trades in the last 7 days.
pub fn liquidity_score(depth: u32, listing_ages_hours: &[f64], volume_7d: u32) -> f64 {
    let depth_score = diminishing(depth as f64, 100.0) * 40.0;

    let freshness_score = if listing_ages_hours.is_empty() {
        0.0
    } else {
        let avg_age = listing_ages_hours.iter().sum::<f64>() / listing_ages_hours.len() as f64;
        (1.0 - (avg_age / HOURS_PER_WEEK).min(1.0)) * 30.0
    };

    let volume_score = diminishing(volume_7d as f64, 100.0) * 30.0;

    (depth_score + freshness_score + volume_score).clamp(0.0, 100.0)
}

/// Demand: how eager buyers currently are. Weighted from three components
/// (buy-side depth 35, growth 35, sale velocity 30):
/// - `buy_depth`: number of active buy orders.
/// - `buy_growth_pct`: percent change in buy depth over some prior window
///   (e.g. week-over-week) — positive means more buyers showing up.
/// - `sale_velocity_per_day`: how many actually sell per day, the real
///   signal that demand converts into trades rather than just listings.
pub fn demand_score(buy_depth: u32, buy_growth_pct: f64, sale_velocity_per_day: f64) -> f64 {
    let depth_score = diminishing(buy_depth as f64, 100.0) * 35.0;

    let growth_score = ((buy_growth_pct.clamp(-100.0, 100.0) + 100.0) / 200.0) * 35.0;

    let velocity_score = (sale_velocity_per_day.max(0.0) / 10.0).min(1.0) * 30.0;

    (depth_score + growth_score + velocity_score).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liquidity_score_is_zero_for_no_activity() {
        assert_eq!(liquidity_score(0, &[], 0), 0.0);
    }

    #[test]
    fn liquidity_score_is_bounded_at_100() {
        let ages = vec![0.0; 50];
        let score = liquidity_score(10_000, &ages, 10_000);
        assert!(score <= 100.0);
    }

    #[test]
    fn liquidity_score_increases_with_depth() {
        let low = liquidity_score(2, &[10.0], 5);
        let high = liquidity_score(50, &[10.0], 5);
        assert!(high > low);
    }

    #[test]
    fn liquidity_score_increases_with_volume() {
        let low = liquidity_score(10, &[10.0], 1);
        let high = liquidity_score(10, &[10.0], 50);
        assert!(high > low);
    }

    #[test]
    fn liquidity_score_decreases_with_older_listings() {
        let fresh = liquidity_score(10, &[1.0, 2.0], 10);
        let stale = liquidity_score(10, &[200.0, 300.0], 10);
        assert!(fresh > stale);
    }

    #[test]
    fn liquidity_score_never_negative() {
        assert!(liquidity_score(0, &[10_000.0], 0) >= 0.0);
    }

    #[test]
    fn demand_score_is_midrange_for_flat_growth_and_no_activity() {
        // buy_growth_pct=0 maps to the midpoint of the growth component
        // even with zero depth/velocity.
        let score = demand_score(0, 0.0, 0.0);
        assert!((score - 17.5).abs() < 1e-9); // 35 (growth range) / 2
    }

    #[test]
    fn demand_score_increases_with_buy_depth() {
        let low = demand_score(2, 0.0, 1.0);
        let high = demand_score(50, 0.0, 1.0);
        assert!(high > low);
    }

    #[test]
    fn demand_score_increases_with_growth() {
        let shrinking = demand_score(10, -50.0, 1.0);
        let growing = demand_score(10, 50.0, 1.0);
        assert!(growing > shrinking);
    }

    #[test]
    fn demand_score_increases_with_sale_velocity() {
        let slow = demand_score(10, 0.0, 0.5);
        let fast = demand_score(10, 0.0, 8.0);
        assert!(fast > slow);
    }

    #[test]
    fn demand_score_is_bounded_0_to_100() {
        assert!(demand_score(100_000, 1000.0, 1000.0) <= 100.0);
        assert!(demand_score(0, -1000.0, 0.0) >= 0.0);
    }
}
