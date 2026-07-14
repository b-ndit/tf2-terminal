use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::steam_id::SteamId64;
use crate::error::{AppError, AppResult};

const CONFIG_FILE_NAME: &str = "config.toml";
const DB_FILE_NAME: &str = "tf2-terminal.db";

/// User-editable application configuration, persisted as TOML in the OS
/// config directory. Never holds secrets — those live in the OS keychain
/// ([`crate::infra::keychain`]). `steam_id` is public/non-secret (see
/// `docs/DESIGN.md` §2) so it's fine here, set after a successful OpenID
/// login.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct Config {
    pub log_level: String,
    pub market_poll_interval_secs: u32,
    pub inventory_refresh_interval_secs: u32,
    pub steam_id: Option<SteamId64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            market_poll_interval_secs: 60,
            inventory_refresh_interval_secs: 300,
            steam_id: None,
        }
    }
}

/// Resolved filesystem locations the app reads/writes, derived from the OS
/// config/data directories via `dirs`.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> AppResult<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::Config("could not resolve OS config directory".into()))?
            .join("tf2-terminal");
        let data_dir = dirs::data_dir()
            .ok_or_else(|| AppError::Config("could not resolve OS data directory".into()))?
            .join("tf2-terminal");
        Ok(Self {
            config_dir,
            data_dir,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE_NAME)
    }

    pub fn db_file(&self) -> PathBuf {
        self.data_dir.join(DB_FILE_NAME)
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}

impl Config {
    /// Loads config from `path`, writing and returning the default if the
    /// file doesn't exist yet.
    pub fn load_or_init(path: &Path) -> AppResult<Self> {
        if !path.exists() {
            let config = Config::default();
            config.save(path)?;
            return Ok(config);
        }
        let raw = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&raw)?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_or_init_creates_default_config_file() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");

        let loaded = Config::load_or_init(&path).unwrap();

        assert_eq!(loaded.log_level, "info");
        assert!(path.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_or_init_reads_existing_config() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "log_level = \"debug\"\n").unwrap();

        let loaded = Config::load_or_init(&path).unwrap();

        assert_eq!(loaded.log_level, "debug");
        assert_eq!(loaded.market_poll_interval_secs, 60);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tf2-terminal-test-{}-{}",
            std::process::id(),
            uniq_suffix()
        ));
        dir
    }

    fn uniq_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}
