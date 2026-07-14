//! Flip opportunity scoring. Pure — every input is already-computed market
//! data (buy/sell prices, liquidity, history depth, an estimated sale
//! time); this module only turns those into a ranked opportunity.
//!
//! Delivered in Module 11 per the roadmap but not called from application
//! code until `services::flip_finder` wires it up — fully exercised by
//! unit tests until then.
#![allow(dead_code)]

/// A pre-valued candidate item to score for a flip. `buy_price_ref` is
/// what it costs to acquire right now (the lowest current sell listing);
/// `sell_price_ref` is the realistic resale target (`estimate_sale_price`,
/// `domain::pricing`) — not the instant-exit quicksell price, which the
/// caller may still want to surface alongside this for comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlipCandidate {
    pub buy_price_ref: f64,
    pub sell_price_ref: f64,
    pub liquidity: f64,
    pub history_days: u32,
    /// Mean age of the item's current sell listings, in hours — a
    /// steady-state proxy for how long a flip typically takes to turn
    /// over (`None` if there are no current sell listings to measure).
    pub est_sale_time_hours: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlipOpportunity {
    pub expected_profit_ref: f64,
    pub roi_pct: f64,
    pub confidence: f64,
    pub est_sale_time_hours: Option<f64>,
}

/// How many days of `price_daily` history it takes for the confidence
/// score's history-depth component to saturate — beyond this, more
/// history doesn't add more confidence.
const HISTORY_DEPTH_CAP_DAYS: f64 = 30.0;

/// Diminishing-returns scaling, same shape as `domain::liquidity`'s
/// private helper of the same name — duplicated rather than shared across
/// modules for a 3-line pure function (matches this codebase's existing
/// per-file `now_unix`/`DAY_SECONDS` duplication convention).
fn diminishing(x: f64, cap: f64) -> f64 {
    (x.max(0.0).sqrt() / cap.sqrt()).min(1.0)
}

/// Scores a flip candidate. `None` if `buy_price_ref` isn't a usable
/// positive price (nothing to divide ROI by) — mirrors
/// `domain::pricing::spread`'s own zero-guard. Doesn't filter on
/// profitability itself — an unprofitable flip still scores (with
/// negative `roi_pct`), the same "always compute, let the caller decide
/// what to show" shape as `domain::trade_rating::rate_trade`.
pub fn score_flip(candidate: &FlipCandidate) -> Option<FlipOpportunity> {
    if candidate.buy_price_ref <= 0.0 {
        return None;
    }

    let expected_profit_ref = candidate.sell_price_ref - candidate.buy_price_ref;
    let roi_pct = expected_profit_ref / candidate.buy_price_ref * 100.0;

    let depth_component = diminishing(candidate.history_days as f64, HISTORY_DEPTH_CAP_DAYS);
    let liquidity_component = (candidate.liquidity / 100.0).clamp(0.0, 1.0);
    let confidence = (depth_component * 0.5 + liquidity_component * 0.5) * 100.0;

    Some(FlipOpportunity {
        expected_profit_ref,
        roi_pct,
        confidence,
        est_sale_time_hours: candidate.est_sale_time_hours,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(buy: f64, sell: f64, liquidity: f64, history_days: u32) -> FlipCandidate {
        FlipCandidate {
            buy_price_ref: buy,
            sell_price_ref: sell,
            liquidity,
            history_days,
            est_sale_time_hours: Some(12.0),
        }
    }

    #[test]
    fn profitable_flip_has_positive_profit_and_roi() {
        let opportunity = score_flip(&candidate(50.0, 65.0, 80.0, 30)).unwrap();
        assert!((opportunity.expected_profit_ref - 15.0).abs() < 1e-9);
        assert!((opportunity.roi_pct - 30.0).abs() < 1e-9);
    }

    #[test]
    fn unprofitable_flip_has_negative_profit_and_roi_but_still_scores() {
        let opportunity = score_flip(&candidate(65.0, 50.0, 80.0, 30)).unwrap();
        assert!((opportunity.expected_profit_ref - (-15.0)).abs() < 1e-9);
        assert!(opportunity.roi_pct < 0.0);
    }

    #[test]
    fn is_none_for_zero_or_negative_buy_price() {
        assert_eq!(score_flip(&candidate(0.0, 10.0, 50.0, 10)), None);
        assert_eq!(score_flip(&candidate(-5.0, 10.0, 50.0, 10)), None);
    }

    #[test]
    fn confidence_increases_with_history_depth() {
        let shallow = score_flip(&candidate(10.0, 15.0, 50.0, 1)).unwrap();
        let deep = score_flip(&candidate(10.0, 15.0, 50.0, 30)).unwrap();
        assert!(deep.confidence > shallow.confidence);
    }

    #[test]
    fn confidence_increases_with_liquidity() {
        let illiquid = score_flip(&candidate(10.0, 15.0, 10.0, 10)).unwrap();
        let liquid = score_flip(&candidate(10.0, 15.0, 90.0, 10)).unwrap();
        assert!(liquid.confidence > illiquid.confidence);
    }

    #[test]
    fn confidence_saturates_beyond_the_cap() {
        let at_cap = score_flip(&candidate(10.0, 15.0, 100.0, 30)).unwrap();
        let past_cap = score_flip(&candidate(10.0, 15.0, 100.0, 365)).unwrap();
        assert!((at_cap.confidence - past_cap.confidence).abs() < 1e-9);
    }

    #[test]
    fn confidence_is_always_within_0_to_100() {
        let min = score_flip(&candidate(10.0, 15.0, 0.0, 0)).unwrap();
        let max = score_flip(&candidate(10.0, 15.0, 100.0, 3650)).unwrap();
        assert!(min.confidence >= 0.0 && min.confidence <= 100.0);
        assert!(max.confidence >= 0.0 && max.confidence <= 100.0);
        assert!((max.confidence - 100.0).abs() < 1e-9);
    }

    #[test]
    fn est_sale_time_hours_passes_through_unchanged() {
        let with_estimate = score_flip(&candidate(10.0, 15.0, 50.0, 10)).unwrap();
        assert_eq!(with_estimate.est_sale_time_hours, Some(12.0));

        let mut no_estimate = candidate(10.0, 15.0, 50.0, 10);
        no_estimate.est_sale_time_hours = None;
        assert_eq!(score_flip(&no_estimate).unwrap().est_sale_time_hours, None);
    }
}
