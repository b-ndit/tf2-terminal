//! Portfolio P/L math. Pure — the caller supplies both the current and
//! past valuation explicitly (from `portfolio_snapshots`); this module
//! only computes the change between them.
//!
//! Delivered in Module 12 per the roadmap but not called from application
//! code until `services::portfolio_service` wires it up — fully exercised
//! by unit tests until then.
#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlWindow {
    pub abs_ref: f64,
    pub pct: f64,
}

/// The change from `past_ref` to `current_ref`. `None` if `past_ref` isn't
/// a usable positive baseline (nothing to compute a percent change
/// against) — mirrors `domain::pricing::spread`'s own zero-guard.
pub fn pl_window(current_ref: f64, past_ref: f64) -> Option<PlWindow> {
    if past_ref <= 0.0 {
        return None;
    }
    let abs_ref = current_ref - past_ref;
    Some(PlWindow {
        abs_ref,
        pct: abs_ref / past_ref * 100.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_change_reports_gain() {
        let window = pl_window(120.0, 100.0).unwrap();
        assert!((window.abs_ref - 20.0).abs() < 1e-9);
        assert!((window.pct - 20.0).abs() < 1e-9);
    }

    #[test]
    fn negative_change_reports_loss() {
        let window = pl_window(80.0, 100.0).unwrap();
        assert!((window.abs_ref - (-20.0)).abs() < 1e-9);
        assert!((window.pct - (-20.0)).abs() < 1e-9);
    }

    #[test]
    fn no_change_is_zero() {
        let window = pl_window(100.0, 100.0).unwrap();
        assert_eq!(window.abs_ref, 0.0);
        assert_eq!(window.pct, 0.0);
    }

    #[test]
    fn is_none_for_zero_or_negative_baseline() {
        assert_eq!(pl_window(100.0, 0.0), None);
        assert_eq!(pl_window(100.0, -5.0), None);
    }
}
