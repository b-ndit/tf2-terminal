//! Module 14's WASM plugin host — `docs/DESIGN.md` §8. `runtime` owns the
//! `wasmtime::Engine`/compiled-module cache and the guest-memory ABI
//! conventions; `host_functions` implements the capability-gated `host_*`
//! imports every plugin links against. `domain::plugin` (pure) owns
//! manifest parsing/validation; `services::plugin_service` owns the
//! install/enable/dispatch lifecycle that ties this to the rest of the app.

pub mod host_functions;
pub mod runtime;
