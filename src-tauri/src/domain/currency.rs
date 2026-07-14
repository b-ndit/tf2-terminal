//! Pure currency math. Delivered in Module 2 per the roadmap but not called
//! from application code until pricing/valuation lands (Module 6 Analytics
//! Engine onward) — fully exercised by unit tests until then.
#![allow(dead_code)]

use std::ops::{Add, Sub};

use serde::{Deserialize, Serialize};
use specta::Type;

/// The live key↔ref exchange rate. A newtype instead of a raw `f64` so
/// "divide by the rate" call sites can't silently divide by zero or NaN.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Type)]
pub struct KeyRate(f64);

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum CurrencyError {
    #[error("key rate must be positive and finite, got {0}")]
    InvalidKeyRate(f64),
}

impl KeyRate {
    pub fn new(ref_per_key: f64) -> Result<Self, CurrencyError> {
        if ref_per_key.is_finite() && ref_per_key > 0.0 {
            Ok(Self(ref_per_key))
        } else {
            Err(CurrencyError::InvalidKeyRate(ref_per_key))
        }
    }

    pub fn ref_per_key(&self) -> f64 {
        self.0
    }
}

/// An amount of TF2 currency. Per `docs/DESIGN.md` §2, all values normalize
/// internally to refined metal: `keys` and `metal_ref` are stored separately
/// (both as floats — this app values/estimates prices, it does not move
/// real currency) and combined into a single ref value only via an explicit
/// [`KeyRate`], since that rate drifts over time and must never be baked in
/// silently.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Currency {
    pub keys: f64,
    pub metal_ref: f64,
}

impl Currency {
    pub const ZERO: Currency = Currency {
        keys: 0.0,
        metal_ref: 0.0,
    };

    pub fn new(keys: f64, metal_ref: f64) -> Self {
        Self { keys, metal_ref }
    }

    pub fn from_ref(metal_ref: f64) -> Self {
        Self {
            keys: 0.0,
            metal_ref,
        }
    }

    /// Total value expressed purely in refined metal.
    pub fn value_in_ref(&self, rate: KeyRate) -> f64 {
        self.keys * rate.ref_per_key() + self.metal_ref
    }

    /// Splits a flat ref amount into a whole number of keys plus a metal
    /// remainder, at the given rate. Useful for display ("that's ~3 keys,
    /// 12.5 ref") — not for anything requiring bit-for-bit precision.
    pub fn from_total_ref(total_ref: f64, rate: KeyRate) -> Self {
        let keys = (total_ref / rate.ref_per_key()).floor();
        let metal_ref = total_ref - keys * rate.ref_per_key();
        Self { keys, metal_ref }
    }
}

impl Add for Currency {
    type Output = Currency;

    fn add(self, rhs: Self) -> Self::Output {
        Currency {
            keys: self.keys + rhs.keys,
            metal_ref: self.metal_ref + rhs.metal_ref,
        }
    }
}

impl Sub for Currency {
    type Output = Currency;

    fn sub(self, rhs: Self) -> Self::Output {
        Currency {
            keys: self.keys - rhs.keys,
            metal_ref: self.metal_ref - rhs.metal_ref,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_rate_rejects_non_positive_and_non_finite() {
        assert!(KeyRate::new(0.0).is_err());
        assert!(KeyRate::new(-1.0).is_err());
        assert!(KeyRate::new(f64::NAN).is_err());
        assert!(KeyRate::new(f64::INFINITY).is_err());
        assert!(KeyRate::new(63.55).is_ok());
    }

    #[test]
    fn value_in_ref_combines_keys_and_metal() {
        let rate = KeyRate::new(60.0).unwrap();
        let c = Currency::new(2.0, 5.5);
        assert_eq!(c.value_in_ref(rate), 125.5);
    }

    #[test]
    fn value_in_ref_metal_only() {
        let rate = KeyRate::new(60.0).unwrap();
        let c = Currency::from_ref(12.33);
        assert_eq!(c.value_in_ref(rate), 12.33);
    }

    #[test]
    fn from_total_ref_splits_whole_keys_and_remainder() {
        let rate = KeyRate::new(60.0).unwrap();
        let c = Currency::from_total_ref(125.5, rate);
        assert_eq!(c.keys, 2.0);
        assert!((c.metal_ref - 5.5).abs() < 1e-9);
    }

    #[test]
    fn from_total_ref_below_one_key_is_all_metal() {
        let rate = KeyRate::new(60.0).unwrap();
        let c = Currency::from_total_ref(45.0, rate);
        assert_eq!(c.keys, 0.0);
        assert_eq!(c.metal_ref, 45.0);
    }

    #[test]
    fn from_total_ref_round_trips_through_value_in_ref() {
        let rate = KeyRate::new(63.55).unwrap();
        let total = 342.17;
        let c = Currency::from_total_ref(total, rate);
        assert!((c.value_in_ref(rate) - total).abs() < 1e-9);
    }

    #[test]
    fn add_and_sub_are_componentwise() {
        let a = Currency::new(1.0, 2.0);
        let b = Currency::new(3.0, 4.5);
        assert_eq!(a + b, Currency::new(4.0, 6.5));
        assert_eq!(b - a, Currency::new(2.0, 2.5));
    }

    #[test]
    fn zero_is_additive_identity() {
        let a = Currency::new(1.5, 7.25);
        assert_eq!(a + Currency::ZERO, a);
        assert_eq!(a - Currency::ZERO, a);
    }
}
