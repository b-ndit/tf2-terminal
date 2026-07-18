# TF2 Terminal

## Project spec
Read `docs/DESIGN.md` for the full architecture, stack decisions,
folder structure, database schemas, service contracts, and
implementation roadmap.

## Rules
- Follow the 15-module roadmap in order. Complete one module fully
  before starting the next.
- Tauri 2.x + React 18 + TypeScript (strict) + Rust backend + SQLite
- No `unwrap()` outside tests
- Every IPC command returns Result<T, AppError>
- Domain layer (src-tauri/src/domain/) has zero I/O — pure functions only
- All external API calls go through rate-limited infra clients
- Secrets in OS keychain only, never in config/DB/logs
- No file over ~400 lines
- Unit tests for all domain logic

## Current status
Modules 1–15 complete (Foundation through Power User + Polish). All 15
roadmap modules shipped — see `docs/RELEASE.md` for packaging/release.
