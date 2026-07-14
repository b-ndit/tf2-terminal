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
use crate::telemetry;

/// Per `docs/DESIGN.md` §6's Module 8 implementation note: periodic
/// snapshots of `market_listings` every 15 minutes.
const PRICE_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// DI container: every service/command reaches shared infrastructure
/// (config, DB pool, paths) through this, managed as Tauri state.
///
/// Holds the tracing [`WorkerGuard`] so the file log writer flushes for the
/// whole process lifetime — it lives and dies with the managed state.
pub struct AppState {
    pub config: RwLock<Config>,
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

    let market_data = Arc::new(MarketDataService::new());
    market_data.spawn_listener(db.clone());
    HistoryRecorder::spawn_periodic(db.clone(), PRICE_SNAPSHOT_INTERVAL);

    Ok(AppState {
        config: RwLock::new(config),
        db,
        paths,
        steam_api: SteamApiClient::new(),
        market_data,
        _log_guard,
    })
}
