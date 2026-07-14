//! `NotificationSink` implementations (`docs/DESIGN.md` §6/§8). Plain
//! functions rather than a `dyn NotificationSink` trait object — there are
//! only two concrete Rust-side sinks and `AlertService` dispatches by
//! matching a rule's `channels` list against fixed string values, so a
//! trait would add indirection without an actual second caller to justify
//! it. The third documented channel, "sound", is deliberately *not* here:
//! see the Module 10 deviation note in `docs/DESIGN.md` §6 — it plays
//! client-side instead.

pub mod discord;
pub mod os;
