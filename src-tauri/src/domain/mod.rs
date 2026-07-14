//! Pure domain logic — zero I/O. Every function here is a plain
//! transformation over structs, which is what makes the analytics engine
//! trivially unit-testable (see `docs/DESIGN.md` §3).

pub mod alerts;
pub mod classified_url;
pub mod currency;
pub mod flip_finder;
pub mod item;
pub mod liquidity;
pub mod pricing;
pub mod steam_id;
pub mod trade_rating;
pub mod trend;
