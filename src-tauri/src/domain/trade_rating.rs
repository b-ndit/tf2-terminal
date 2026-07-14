//! Trade offer valuation and ★ rating. Pure — every input is an already-
//! valued item (spread/liquidity/demand/estimate come from
//! `domain::pricing`/`domain::liquidity`, computed by the caller against
//! live market data); this module only combines those into a verdict.
//!
//! Rule-based and explainable by design (`docs/DESIGN.md` §12): the
//! ★ rating is two weighted, capped components (value 0..60, risk 0..40)
//! bucketed into stars, and `explanation` is template-composed from the
//! same numbers that drove the score — no hidden factors.
#![allow(dead_code)]

/// One item on either side of a trade, already valued by the caller.
/// `estimated_ref` (what it would likely sell for) is preferred over
/// `quicksell_ref` (fastest-offload price) for totals; either may be
/// `None` if the item couldn't be resolved against market data at all
/// (unknown SKU, no listings, schema not synced).
#[derive(Debug, Clone, PartialEq)]
pub struct ValuedItem {
    pub name: String,
    pub estimated_ref: Option<f64>,
    pub quicksell_ref: Option<f64>,
    pub liquidity: f64,
    pub demand: f64,
    pub spread_pct: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TradeSide {
    pub items: Vec<ValuedItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CounterofferSuggestion {
    /// How much more ref you'd need to receive to bring the trade to
    /// breakeven (`net_ref == 0`).
    pub additional_ref_needed: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeVerdict {
    pub stars: u8,
    pub given_total_ref: f64,
    pub received_total_ref: f64,
    pub net_ref: f64,
    /// `None` when `given_total_ref` is zero (ROI is undefined when you're
    /// giving up nothing).
    pub roi_pct: Option<f64>,
    pub risk: RiskLevel,
    pub explanation: Vec<String>,
    /// `Some` only when the trade is currently unfavorable (`net_ref < 0`)
    /// and there's a nonzero given side to measure against.
    pub counteroffer: Option<CounterofferSuggestion>,
    pub unpriced_given: u32,
    pub unpriced_received: u32,
}

/// Sums the priced items on a side (`estimated_ref`, falling back to
/// `quicksell_ref` if that's all that's available) and counts how many
/// items had neither — those don't contribute to the total, which biases
/// an all-unpriced side toward looking "free" rather than expensive, so
/// callers should treat a high `unpriced_*` count as reduced confidence,
/// not a good deal.
fn side_total(side: &TradeSide) -> (f64, u32) {
    let mut total = 0.0;
    let mut unpriced = 0;
    for item in &side.items {
        match item.estimated_ref.or(item.quicksell_ref) {
            Some(value) => total += value,
            None => unpriced += 1,
        }
    }
    (total, unpriced)
}

fn avg(values: impl Iterator<Item = f64> + Clone) -> f64 {
    let count = values.clone().count();
    if count == 0 {
        return 0.0;
    }
    values.sum::<f64>() / count as f64
}

fn avg_spread_pct(side: &TradeSide) -> Option<f64> {
    let spreads: Vec<f64> = side.items.iter().filter_map(|i| i.spread_pct).collect();
    if spreads.is_empty() {
        None
    } else {
        Some(spreads.iter().sum::<f64>() / spreads.len() as f64)
    }
}

/// Rates a trade offer by valuing both sides. `given` is what the user
/// would hand over; `received` is what they'd get. Risk is judged purely
/// off the *received* side — how easily could you turn what you're about
/// to receive back into something else, if you wanted to.
pub fn rate_trade(given: &TradeSide, received: &TradeSide) -> TradeVerdict {
    let (given_total_ref, unpriced_given) = side_total(given);
    let (received_total_ref, unpriced_received) = side_total(received);
    let net_ref = received_total_ref - given_total_ref;
    let roi_pct = if given_total_ref > 0.0 {
        Some(net_ref / given_total_ref * 100.0)
    } else {
        None
    };

    let avg_liquidity = avg(received.items.iter().map(|i| i.liquidity));
    let avg_demand = avg(received.items.iter().map(|i| i.demand));
    let avg_spread = avg_spread_pct(received);

    // Blend of received-side liquidity/demand, 0..100. An empty received
    // side naturally lands at 0 here (both `avg()` calls return 0.0 for no
    // items), which correctly falls into the High-risk bucket below rather
    // than needing a special case.
    let risk_composite = avg_liquidity * 0.5 + avg_demand * 0.5;
    let risk = if risk_composite < 35.0 {
        RiskLevel::High
    } else if risk_composite < 65.0 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    // Value component (0..60): linear in ROI, clamped to +/-60% so one
    // extreme outlier item can't single-handedly saturate the score.
    // Missing ROI (nothing given up) scores as neutral (30/60) rather than
    // being treated as either great or terrible.
    let roi_for_scoring = roi_pct.unwrap_or(0.0).clamp(-60.0, 60.0);
    let value_score = ((roi_for_scoring + 60.0) / 120.0) * 60.0;
    // Risk component (0..40): the same received-side composite used for
    // the `risk` bucket above, just rescaled.
    let risk_score = (risk_composite / 100.0) * 40.0;
    let total_score = (value_score + risk_score).clamp(0.0, 100.0);
    // 5 even buckets over 0..100 -> stars 1..5.
    let stars = 1 + (total_score / 20.0).floor().min(4.0) as u8;

    let counteroffer = if net_ref < 0.0 && given_total_ref > 0.0 {
        Some(CounterofferSuggestion {
            additional_ref_needed: -net_ref,
        })
    } else {
        None
    };

    let explanation = build_explanation(ExplanationInput {
        given,
        received,
        given_total_ref,
        received_total_ref,
        net_ref,
        roi_pct,
        avg_liquidity,
        avg_demand,
        avg_spread,
        unpriced_given,
        unpriced_received,
        counteroffer: &counteroffer,
    });

    TradeVerdict {
        stars,
        given_total_ref,
        received_total_ref,
        net_ref,
        roi_pct,
        risk,
        explanation,
        counteroffer,
        unpriced_given,
        unpriced_received,
    }
}

struct ExplanationInput<'a> {
    given: &'a TradeSide,
    received: &'a TradeSide,
    given_total_ref: f64,
    received_total_ref: f64,
    net_ref: f64,
    roi_pct: Option<f64>,
    avg_liquidity: f64,
    avg_demand: f64,
    avg_spread: Option<f64>,
    unpriced_given: u32,
    unpriced_received: u32,
    counteroffer: &'a Option<CounterofferSuggestion>,
}

/// The threshold above which a wide sell-side spread on the received
/// items is called out — below this it's normal market noise, not
/// something worth flagging.
const WIDE_SPREAD_PCT: f64 = 15.0;

fn build_explanation(input: ExplanationInput) -> Vec<String> {
    let mut lines = Vec::new();

    lines.push(format!(
        "You give {} item(s) worth an estimated {:.2} ref",
        input.given.items.len(),
        input.given_total_ref
    ));
    lines.push(format!(
        "You receive {} item(s) worth an estimated {:.2} ref",
        input.received.items.len(),
        input.received_total_ref
    ));

    match input.roi_pct {
        Some(roi) => lines.push(format!(
            "Net value: {:+.2} ref ({:+.1}% ROI)",
            input.net_ref, roi
        )),
        None => lines.push(format!("Net value: {:+.2} ref", input.net_ref)),
    }

    if input.received.items.is_empty() {
        lines.push("You receive no items — nothing to evaluate on that side".to_string());
    } else {
        lines.push(format!(
            "Received-side liquidity {:.0}/100, demand {:.0}/100",
            input.avg_liquidity, input.avg_demand
        ));
    }

    if let Some(spread) = input.avg_spread {
        if spread > WIDE_SPREAD_PCT {
            lines.push(format!(
                "Sell-side spread on received items is wide (~{spread:.1}%) — may be slow or costly to resell"
            ));
        }
    }

    if input.unpriced_given > 0 {
        lines.push(format!(
            "{} given item(s) could not be priced — verdict confidence reduced",
            input.unpriced_given
        ));
    }
    if input.unpriced_received > 0 {
        lines.push(format!(
            "{} received item(s) could not be priced — verdict confidence reduced",
            input.unpriced_received
        ));
    }

    if let Some(c) = input.counteroffer {
        lines.push(format!(
            "Consider asking for an additional ~{:.2} ref to break even",
            c.additional_ref_needed
        ));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(estimated_ref: f64, liquidity: f64, demand: f64) -> ValuedItem {
        ValuedItem {
            name: "Test Item".to_string(),
            estimated_ref: Some(estimated_ref),
            quicksell_ref: Some(estimated_ref * 0.9),
            liquidity,
            demand,
            spread_pct: Some(5.0),
        }
    }

    fn side(items: Vec<ValuedItem>) -> TradeSide {
        TradeSide { items }
    }

    #[test]
    fn favorable_liquid_trade_gets_high_stars_and_positive_net() {
        let given = side(vec![item(50.0, 80.0, 80.0)]);
        let received = side(vec![item(80.0, 80.0, 80.0)]);

        let verdict = rate_trade(&given, &received);

        assert!((verdict.net_ref - 30.0).abs() < 1e-9);
        assert!((verdict.roi_pct.unwrap() - 60.0).abs() < 1e-9);
        assert_eq!(verdict.risk, RiskLevel::Low);
        assert_eq!(verdict.stars, 5);
        assert_eq!(verdict.counteroffer, None);
    }

    #[test]
    fn unfavorable_trade_gets_low_stars_negative_net_and_counteroffer() {
        let given = side(vec![item(100.0, 50.0, 50.0)]);
        let received = side(vec![item(40.0, 20.0, 20.0)]);

        let verdict = rate_trade(&given, &received);

        assert!(verdict.net_ref < 0.0);
        assert!(verdict.roi_pct.unwrap() < 0.0);
        assert_eq!(verdict.risk, RiskLevel::High);
        assert_eq!(verdict.stars, 1);
        let counter = verdict
            .counteroffer
            .expect("unfavorable trade should suggest a counteroffer");
        assert!((counter.additional_ref_needed - 60.0).abs() < 1e-9);
    }

    #[test]
    fn break_even_trade_has_no_counteroffer() {
        let given = side(vec![item(50.0, 50.0, 50.0)]);
        let received = side(vec![item(50.0, 50.0, 50.0)]);

        let verdict = rate_trade(&given, &received);

        assert!((verdict.net_ref).abs() < 1e-9);
        assert_eq!(verdict.counteroffer, None);
    }

    #[test]
    fn empty_sides_do_not_panic() {
        let verdict = rate_trade(&TradeSide::default(), &TradeSide::default());
        assert_eq!(verdict.given_total_ref, 0.0);
        assert_eq!(verdict.received_total_ref, 0.0);
        assert_eq!(verdict.roi_pct, None);
        assert_eq!(verdict.risk, RiskLevel::High);
        assert_eq!(verdict.counteroffer, None);
        assert!(verdict.stars >= 1 && verdict.stars <= 5);
    }

    #[test]
    fn unpriced_items_are_excluded_from_totals_but_counted() {
        let unpriced = ValuedItem {
            name: "Mystery Item".to_string(),
            estimated_ref: None,
            quicksell_ref: None,
            liquidity: 0.0,
            demand: 0.0,
            spread_pct: None,
        };
        let given = side(vec![item(10.0, 50.0, 50.0), unpriced.clone()]);
        let received = side(vec![item(10.0, 50.0, 50.0), unpriced]);

        let verdict = rate_trade(&given, &received);

        assert_eq!(verdict.given_total_ref, 10.0);
        assert_eq!(verdict.received_total_ref, 10.0);
        assert_eq!(verdict.unpriced_given, 1);
        assert_eq!(verdict.unpriced_received, 1);
        assert!(verdict
            .explanation
            .iter()
            .any(|line| line.contains("could not be priced")));
    }

    #[test]
    fn quicksell_is_used_when_estimated_is_missing() {
        let fallback = ValuedItem {
            name: "Fallback Item".to_string(),
            estimated_ref: None,
            quicksell_ref: Some(25.0),
            liquidity: 40.0,
            demand: 40.0,
            spread_pct: None,
        };
        let received = side(vec![fallback]);
        let verdict = rate_trade(&TradeSide::default(), &received);
        assert_eq!(verdict.received_total_ref, 25.0);
        assert_eq!(verdict.unpriced_received, 0);
    }

    #[test]
    fn risk_level_reflects_received_side_liquidity_and_demand() {
        let low_risk = rate_trade(&TradeSide::default(), &side(vec![item(10.0, 90.0, 90.0)]));
        let medium_risk = rate_trade(&TradeSide::default(), &side(vec![item(10.0, 50.0, 50.0)]));
        let high_risk = rate_trade(&TradeSide::default(), &side(vec![item(10.0, 10.0, 10.0)]));

        assert_eq!(low_risk.risk, RiskLevel::Low);
        assert_eq!(medium_risk.risk, RiskLevel::Medium);
        assert_eq!(high_risk.risk, RiskLevel::High);
    }

    #[test]
    fn wide_spread_on_received_side_is_called_out() {
        let wide_spread_item = ValuedItem {
            spread_pct: Some(40.0),
            ..item(10.0, 50.0, 50.0)
        };
        let verdict = rate_trade(&TradeSide::default(), &side(vec![wide_spread_item]));
        assert!(verdict.explanation.iter().any(|line| line.contains("wide")));
    }

    #[test]
    fn narrow_spread_is_not_called_out() {
        let verdict = rate_trade(&TradeSide::default(), &side(vec![item(10.0, 50.0, 50.0)]));
        assert!(!verdict.explanation.iter().any(|line| line.contains("wide")));
    }

    #[test]
    fn stars_are_always_within_1_to_5() {
        let extreme_good = rate_trade(
            &side(vec![item(1.0, 100.0, 100.0)]),
            &side(vec![item(1000.0, 100.0, 100.0)]),
        );
        let extreme_bad = rate_trade(
            &side(vec![item(1000.0, 100.0, 100.0)]),
            &side(vec![item(1.0, 0.0, 0.0)]),
        );
        assert_eq!(extreme_good.stars, 5);
        assert_eq!(extreme_bad.stars, 1);
    }

    #[test]
    fn risk_level_as_str_is_lowercase() {
        assert_eq!(RiskLevel::Low.as_str(), "low");
        assert_eq!(RiskLevel::Medium.as_str(), "medium");
        assert_eq!(RiskLevel::High.as_str(), "high");
    }
}
