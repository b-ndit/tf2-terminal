//! Alert rule evaluation. Pure — every input is a plain signal the caller
//! already has in hand (a live `ListingEvent`'s shape, or a `price_daily`
//! rollup comparison); this module only decides whether a rule fires.
//!
//! `AlertService` (`docs/DESIGN.md` §6) is "a rule engine subscribing to
//! `ListingEvent` + daily rollups" — [`AlertSignal`]'s two variants mirror
//! that split exactly: the four event-driven kinds react to one listing at
//! a time, the two rollup-driven kinds react to a day closing.
//!
//! Delivered in Module 10 per the roadmap but not called from application
//! code until `services::alert_service` lands later in this same module —
//! fully exercised by unit tests until then.
#![allow(dead_code)]

use crate::domain::pricing::Intent;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown alert kind: {0}")]
pub struct UnknownAlertKind(pub String);

/// The six rule kinds `docs/DESIGN.md` §5's `alert_rules.kind` column
/// documents. `as_str`'s values are the exact strings stored in that
/// column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    PriceDrop,
    SpreadWiden,
    NewBuyer,
    NewSeller,
    HistLow,
    HistHigh,
}

impl AlertKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertKind::PriceDrop => "price_drop",
            AlertKind::SpreadWiden => "spread_widen",
            AlertKind::NewBuyer => "new_buyer",
            AlertKind::NewSeller => "new_seller",
            AlertKind::HistLow => "hist_low",
            AlertKind::HistHigh => "hist_high",
        }
    }

    pub fn parse(s: &str) -> Result<Self, UnknownAlertKind> {
        match s {
            "price_drop" => Ok(AlertKind::PriceDrop),
            "spread_widen" => Ok(AlertKind::SpreadWiden),
            "new_buyer" => Ok(AlertKind::NewBuyer),
            "new_seller" => Ok(AlertKind::NewSeller),
            "hist_low" => Ok(AlertKind::HistLow),
            "hist_high" => Ok(AlertKind::HistHigh),
            other => Err(UnknownAlertKind(other.to_string())),
        }
    }

    /// `price_drop`/`spread_widen` compare against a user-supplied number;
    /// the other four kinds either fire unconditionally (new_buyer/seller)
    /// or compare against historical data, not a threshold the user picks.
    /// Callers creating a rule should require a threshold only for these.
    pub fn requires_threshold(&self) -> bool {
        matches!(self, AlertKind::PriceDrop | AlertKind::SpreadWiden)
    }
}

/// One signal `AlertService` might evaluate rules against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlertSignal {
    /// A single live `ListingEvent` (`docs/DESIGN.md` §6) — `is_new`
    /// distinguishes a brand-new listing from an updated one (Module 7's
    /// `SeenListings` tracker already makes this distinction), and
    /// `current_spread_pct` is `None` unless the caller bothered to
    /// compute it (only needed for `spread_widen` rules).
    Listing {
        is_new: bool,
        intent: Intent,
        price_ref: f64,
        current_spread_pct: Option<f64>,
    },
    /// One item's `price_daily` rollup closing for the day, alongside the
    /// historical extremes from every *prior* day (`None` if there's no
    /// prior history to compare against yet).
    DailyClose {
        close_ref: f64,
        historical_low_ref: Option<f64>,
        historical_high_ref: Option<f64>,
    },
}

/// Whether `signal` should fire a rule of `kind` with the given
/// `threshold` (`None` for kinds that don't use one, or when the rule was
/// created without one — see [`AlertKind::requires_threshold`]).
pub fn evaluate(kind: AlertKind, threshold: Option<f64>, signal: &AlertSignal) -> bool {
    match (kind, signal) {
        (
            AlertKind::PriceDrop,
            AlertSignal::Listing {
                intent: Intent::Sell,
                price_ref,
                ..
            },
        ) => threshold.is_some_and(|t| *price_ref <= t),

        (
            AlertKind::SpreadWiden,
            AlertSignal::Listing {
                current_spread_pct: Some(pct),
                ..
            },
        ) => threshold.is_some_and(|t| *pct >= t),

        (
            AlertKind::NewBuyer,
            AlertSignal::Listing {
                is_new: true,
                intent: Intent::Buy,
                ..
            },
        ) => true,

        (
            AlertKind::NewSeller,
            AlertSignal::Listing {
                is_new: true,
                intent: Intent::Sell,
                ..
            },
        ) => true,

        (
            AlertKind::HistLow,
            AlertSignal::DailyClose {
                close_ref,
                historical_low_ref: Some(low),
                ..
            },
        ) => close_ref <= low,

        (
            AlertKind::HistHigh,
            AlertSignal::DailyClose {
                close_ref,
                historical_high_ref: Some(high),
                ..
            },
        ) => close_ref >= high,

        _ => false,
    }
}

/// A human-readable line describing why a rule fired — template-composed
/// from `kind`/`threshold`/`signal`, the same "deterministic and
/// auditable, not hand-wavy" approach `domain::trade_rating`'s explanation
/// builder uses (`docs/DESIGN.md` §12). Used as both the OS notification
/// body and the Discord message content.
pub fn describe(
    kind: AlertKind,
    item_name: &str,
    threshold: Option<f64>,
    signal: &AlertSignal,
) -> String {
    match (kind, signal) {
        (AlertKind::PriceDrop, AlertSignal::Listing { price_ref, .. }) => format!(
            "{item_name}: new sell at {price_ref:.2} ref (threshold {:.2} ref)",
            threshold.unwrap_or(0.0)
        ),
        (
            AlertKind::SpreadWiden,
            AlertSignal::Listing {
                current_spread_pct: Some(pct),
                ..
            },
        ) => format!(
            "{item_name}: spread widened to {pct:.1}% (threshold {:.1}%)",
            threshold.unwrap_or(0.0)
        ),
        (AlertKind::NewBuyer, _) => format!("{item_name}: a new buyer listed"),
        (AlertKind::NewSeller, _) => format!("{item_name}: a new seller listed"),
        (AlertKind::HistLow, AlertSignal::DailyClose { close_ref, .. }) => {
            format!("{item_name}: new historical low at {close_ref:.2} ref")
        }
        (AlertKind::HistHigh, AlertSignal::DailyClose { close_ref, .. }) => {
            format!("{item_name}: new historical high at {close_ref:.2} ref")
        }
        _ => format!("{item_name}: {} alert", kind.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sell(price_ref: f64) -> AlertSignal {
        AlertSignal::Listing {
            is_new: false,
            intent: Intent::Sell,
            price_ref,
            current_spread_pct: None,
        }
    }

    fn buy(price_ref: f64) -> AlertSignal {
        AlertSignal::Listing {
            is_new: false,
            intent: Intent::Buy,
            price_ref,
            current_spread_pct: None,
        }
    }

    #[test]
    fn kind_round_trips_through_as_str_and_parse() {
        for kind in [
            AlertKind::PriceDrop,
            AlertKind::SpreadWiden,
            AlertKind::NewBuyer,
            AlertKind::NewSeller,
            AlertKind::HistLow,
            AlertKind::HistHigh,
        ] {
            assert_eq!(AlertKind::parse(kind.as_str()).unwrap(), kind);
        }
    }

    #[test]
    fn parse_rejects_unknown_kind() {
        assert!(AlertKind::parse("bogus").is_err());
    }

    #[test]
    fn requires_threshold_is_true_only_for_price_drop_and_spread_widen() {
        assert!(AlertKind::PriceDrop.requires_threshold());
        assert!(AlertKind::SpreadWiden.requires_threshold());
        assert!(!AlertKind::NewBuyer.requires_threshold());
        assert!(!AlertKind::NewSeller.requires_threshold());
        assert!(!AlertKind::HistLow.requires_threshold());
        assert!(!AlertKind::HistHigh.requires_threshold());
    }

    #[test]
    fn price_drop_fires_at_or_below_threshold() {
        assert!(evaluate(AlertKind::PriceDrop, Some(50.0), &sell(50.0)));
        assert!(evaluate(AlertKind::PriceDrop, Some(50.0), &sell(40.0)));
        assert!(!evaluate(AlertKind::PriceDrop, Some(50.0), &sell(60.0)));
    }

    #[test]
    fn price_drop_ignores_buy_listings() {
        assert!(!evaluate(AlertKind::PriceDrop, Some(50.0), &buy(10.0)));
    }

    #[test]
    fn price_drop_never_fires_without_a_threshold() {
        assert!(!evaluate(AlertKind::PriceDrop, None, &sell(0.0)));
    }

    #[test]
    fn spread_widen_fires_at_or_above_threshold() {
        let signal = AlertSignal::Listing {
            is_new: false,
            intent: Intent::Sell,
            price_ref: 10.0,
            current_spread_pct: Some(20.0),
        };
        assert!(evaluate(AlertKind::SpreadWiden, Some(15.0), &signal));
        assert!(evaluate(AlertKind::SpreadWiden, Some(20.0), &signal));
        assert!(!evaluate(AlertKind::SpreadWiden, Some(25.0), &signal));
    }

    #[test]
    fn spread_widen_never_fires_without_a_computed_spread() {
        assert!(!evaluate(AlertKind::SpreadWiden, Some(1.0), &sell(10.0)));
    }

    #[test]
    fn new_buyer_fires_only_for_new_buy_listings() {
        let new_buy = AlertSignal::Listing {
            is_new: true,
            intent: Intent::Buy,
            price_ref: 10.0,
            current_spread_pct: None,
        };
        assert!(evaluate(AlertKind::NewBuyer, None, &new_buy));
        assert!(!evaluate(AlertKind::NewBuyer, None, &buy(10.0))); // is_new: false
        assert!(!evaluate(
            AlertKind::NewBuyer,
            None,
            &AlertSignal::Listing {
                is_new: true,
                intent: Intent::Sell,
                price_ref: 10.0,
                current_spread_pct: None,
            }
        ));
    }

    #[test]
    fn new_seller_fires_only_for_new_sell_listings() {
        let new_sell = AlertSignal::Listing {
            is_new: true,
            intent: Intent::Sell,
            price_ref: 10.0,
            current_spread_pct: None,
        };
        assert!(evaluate(AlertKind::NewSeller, None, &new_sell));
        assert!(!evaluate(AlertKind::NewSeller, None, &sell(10.0))); // is_new: false
    }

    #[test]
    fn hist_low_fires_when_close_is_at_or_below_prior_low() {
        let signal = AlertSignal::DailyClose {
            close_ref: 40.0,
            historical_low_ref: Some(45.0),
            historical_high_ref: Some(100.0),
        };
        assert!(evaluate(AlertKind::HistLow, None, &signal));
    }

    #[test]
    fn hist_low_does_not_fire_above_prior_low() {
        let signal = AlertSignal::DailyClose {
            close_ref: 50.0,
            historical_low_ref: Some(45.0),
            historical_high_ref: Some(100.0),
        };
        assert!(!evaluate(AlertKind::HistLow, None, &signal));
    }

    #[test]
    fn hist_high_fires_when_close_is_at_or_above_prior_high() {
        let signal = AlertSignal::DailyClose {
            close_ref: 120.0,
            historical_low_ref: Some(45.0),
            historical_high_ref: Some(100.0),
        };
        assert!(evaluate(AlertKind::HistHigh, None, &signal));
    }

    #[test]
    fn hist_low_and_high_never_fire_without_prior_history() {
        let signal = AlertSignal::DailyClose {
            close_ref: 40.0,
            historical_low_ref: None,
            historical_high_ref: None,
        };
        assert!(!evaluate(AlertKind::HistLow, None, &signal));
        assert!(!evaluate(AlertKind::HistHigh, None, &signal));
    }

    #[test]
    fn mismatched_kind_and_signal_never_fires() {
        assert!(!evaluate(
            AlertKind::HistLow,
            None,
            &sell(10.0) // a Listing signal, not DailyClose
        ));
        assert!(!evaluate(
            AlertKind::PriceDrop,
            Some(10.0),
            &AlertSignal::DailyClose {
                close_ref: 5.0,
                historical_low_ref: Some(1.0),
                historical_high_ref: Some(100.0),
            }
        ));
    }

    #[test]
    fn describe_price_drop_includes_price_and_threshold() {
        let text = describe(AlertKind::PriceDrop, "Key", Some(50.0), &sell(45.0));
        assert!(text.contains("Key"));
        assert!(text.contains("45.00"));
        assert!(text.contains("50.00"));
    }

    #[test]
    fn describe_spread_widen_includes_pct_and_threshold() {
        let signal = AlertSignal::Listing {
            is_new: false,
            intent: Intent::Sell,
            price_ref: 10.0,
            current_spread_pct: Some(22.5),
        };
        let text = describe(AlertKind::SpreadWiden, "Key", Some(15.0), &signal);
        assert!(text.contains("22.5"));
        assert!(text.contains("15.0"));
    }

    #[test]
    fn describe_new_buyer_and_seller_name_the_item() {
        let signal = AlertSignal::Listing {
            is_new: true,
            intent: Intent::Buy,
            price_ref: 10.0,
            current_spread_pct: None,
        };
        assert!(describe(AlertKind::NewBuyer, "Key", None, &signal).contains("Key"));
        assert!(describe(AlertKind::NewSeller, "Key", None, &signal).contains("Key"));
    }

    #[test]
    fn describe_hist_low_and_high_include_the_close_price() {
        let signal = AlertSignal::DailyClose {
            close_ref: 33.5,
            historical_low_ref: Some(40.0),
            historical_high_ref: Some(60.0),
        };
        assert!(describe(AlertKind::HistLow, "Key", None, &signal).contains("33.50"));
        assert!(describe(AlertKind::HistHigh, "Key", None, &signal).contains("33.50"));
    }
}
