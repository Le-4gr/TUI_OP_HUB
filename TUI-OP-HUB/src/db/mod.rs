//! Database layer (US-NF-06, US-NF-07).
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use crate::config::DatabaseConfig;
use crate::error::{AppError, AppResult};

pub async fn init_pool(cfg: &DatabaseConfig) -> AppResult<SqlitePool> {
    let url = format!("sqlite://{}", cfg.path);
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| AppError::Config(format!("invalid db url: {e}")))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_millis(cfg.busy_timeout_ms));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    tracing::info!(path = %cfg.path, "SQLite pool initialised (WAL, foreign_keys, busy_timeout={}ms)", cfg.busy_timeout_ms);
    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> AppResult<()> {
    sqlx::migrate!("./src/db/migrations")
        .run(pool)
        .await
        .map_err(AppError::Migration)?;
    tracing::info!("database migrations complete");
    Ok(())
}
