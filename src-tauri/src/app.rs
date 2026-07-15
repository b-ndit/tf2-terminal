use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tracing_appender::non_blocking::WorkerGuard;

use crate::error::AppResult;
use crate::infra::config::{AppPaths, Config};
use crate::infra::db;
use crate::infra::steam::SteamApiClient;
use crate::services::history_recorder::HistoryRecorder;
use crate::services::market_data_service::MarketDataService;
use crate::services::portfolio_service;
use crate::telemetry;

/// Per `docs/DESIGN.md` §6's Module 8 implementation note: periodic
/// snapshots of `market_listings` every 15 minutes.
const PRICE_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(15 * 60);
/// Module 12's "daily ... valuation snapshots" (§6) — the "on-demand" half
/// is `commands::portfolio::get_portfolio_snapshot`.
const PORTFOLIO_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// DI container: every service/command reaches shared infrastructure
/// (config, DB pool, paths) through this, managed as Tauri state.
///
/// Holds the tracing [`WorkerGuard`] so the file log writer flushes for the
/// whole process lifetime — it lives and dies with the managed state.
pub struct AppState {
    /// `Arc`-wrapped (not just `RwLock<Config>`) so background tasks
    /// spawned here in `build()` — like `PortfolioService`'s periodic
    /// snapshot loop, Module 12 — can hold their own clone of the *same*
    /// config data the eventually-`.manage()`d `AppState` uses, the same
    /// way `market_data` already does. Transparent to every existing
    /// command: `state.config.read()/.write()` still compiles identically,
    /// since `Arc<RwLock<T>>` auto-derefs to `RwLock<T>`.
    pub config: Arc<RwLock<Config>>,
    pub db: SqlitePool,
    pub paths: AppPaths,
    pub steam_api: SteamApiClient,
    pub market_data: Arc<MarketDataService>,
    _log_guard: WorkerGuard,
}

/// Resolves paths, loads config, initializes tracing, and opens the DB pool
/// (running migrations). Called once at startup before the Tauri builder
/// runs.
pub async fn build() -> AppResult<AppState> {
    let paths = AppPaths::resolve()?;
    let config = Config::load_or_init(&paths.config_file())?;
    let _log_guard = telemetry::init(&paths.log_dir(), &config.log_level)?;

    tracing::info!(
        config_dir = %paths.config_dir.display(),
        data_dir = %paths.data_dir.display(),
        "starting tf2-terminal"
    );

    let db = db::init_pool(&paths.db_file()).await?;

    let config = Arc::new(RwLock::new(config));

    let market_data = Arc::new(MarketDataService::new());
    market_data.spawn_listener(db.clone());
    HistoryRecorder::spawn_periodic(db.clone(), PRICE_SNAPSHOT_INTERVAL);
    portfolio_service::spawn_periodic_snapshot(
        config.clone(),
        db.clone(),
        PORTFOLIO_SNAPSHOT_INTERVAL,
    );

    Ok(AppState {
        config,
        db,
        paths,
        steam_api: SteamApiClient::new(),
        market_data,
        _log_guard,
    })
}
