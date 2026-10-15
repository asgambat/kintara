use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

/// Opens the database, applying the pragmas that matter for a multi-reader
/// server, then runs any pending migrations.
///
/// WAL is what makes concurrent reads viable while a write is in flight; the
/// busy timeout covers the brief windows where it is not.
pub async fn connect(database_path: &Path) -> Result<SqlitePool> {
    let url = format!("sqlite://{}", database_path.display());
    // auto_vacuum lives in the database header, and the set only sticks when
    // done before any table exists, so the fresh-database check happens
    // before opening the file.
    let fresh = !database_path.exists();

    let mut options = SqliteConnectOptions::from_str(&url)
        .with_context(|| format!("invalid database path {}", database_path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));

    // Fresh installs reclaim freed pages incrementally forever after, without
    // ever paying for a full rewrite. Existing databases are untouched on
    // purpose: switching them requires a whole-file VACUUM, which can block
    // for minutes on large libraries (see the README for the manual one-off
    // step). The set is a cheap per-connection no-op once tables exist, so
    // attaching it to the connect options (applied on *every* pool
    // connection) is what makes the flag survive into the header: the first
    // writer — from whichever pooled connection — carries it. Executing the
    // pragma on one pooled connection alone does not, because the schema is
    // then created by other connections whose state is still the default.
    if fresh {
        options = options.pragma("auto_vacuum", "2");
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .context("failed to open the Kintara database")?;

    let migrator = sqlx::migrate!("./migrations");
    if has_pending(&pool, &migrator).await? {
        backup(&pool, database_path).await?;
    }
    migrator
        .run(&pool)
        .await
        .context("failed to apply database migrations")?;

    Ok(pool)
}

/// How many pre-migration backups are kept in `<data_dir>/backups`.
const BACKUP_KEEP: usize = 5;

/// True when the embedded migration list contains versions not yet applied.
/// A missing `_sqlx_migrations` table means a fresh database, which needs no
/// backup because there is nothing to lose yet.
async fn has_pending(pool: &SqlitePool, migrator: &sqlx::migrate::Migrator) -> Result<bool> {
    let applied: HashSet<i64> =
        match sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations")
            .fetch_all(pool)
            .await
        {
            Ok(versions) => versions.into_iter().collect(),
            Err(sqlx::Error::Database(db)) if db.message().contains("no such table") => {
                return Ok(false)
            }
            Err(err) => return Err(err.into()),
        };
    Ok(migrator
        .migrations
        .iter()
        .any(|m| !applied.contains(&m.version)))
}

/// Copy of the database file into `<data_dir>/backups`. Used by the pre-migration
/// guard and by the scheduled snapshot loop (`KINTARA_BACKUP_INTERVAL_HOURS`).
///
/// SQLite's online `VACUUM INTO` reads the source through one implicit read
/// transaction (WAL contents included), so readers and writers are never
/// blocked and the output stands alone. A bare file copy would miss any
/// transaction still living in the WAL and would also drag the WAL/SHM
/// sidecars along. Millisecond timestamp: two snapshots can legally land in
/// the same second.
pub async fn backup(pool: &SqlitePool, database_path: &Path) -> Result<()> {
    let Some(data_dir) = database_path.parent() else {
        anyhow::bail!("database path {} has no parent directory", database_path.display());
    };
    let backups = data_dir.join("backups");
    tokio::fs::create_dir_all(&backups)
        .await
        .with_context(|| format!("cannot create {}", backups.display()))?;

    let stem = database_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("kintara");
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S%.3f");
    let dest = backups.join(format!("{stem}-{stamp}.db"));
    sqlx::query("VACUUM INTO ?")
        .bind(dest.to_string_lossy().to_string())
        .execute(pool)
        .await
        .with_context(|| format!("cannot back up to {}", dest.display()))?;
    tracing::info!(backup = %dest.display(), "database snapshot created");

    prune_backups(database_path).await
}

/// Keeps the newest [`BACKUP_KEEP`] snapshots and prunes the older ones.
async fn prune_backups(database_path: &Path) -> Result<()> {
    let stem = database_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("kintara");
    let Some(data_dir) = database_path.parent() else {
        return Ok(());
    };
    let backups = data_dir.join("backups");

    let mut kept: Vec<std::path::PathBuf> = Vec::new();
    let mut entries = tokio::fs::read_dir(&backups)
        .await
        .with_context(|| format!("cannot list {}", backups.display()))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .with_context(|| format!("cannot list {}", backups.display()))?
    {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let prefix = format!("{stem}-");
        if name.starts_with(prefix.as_str()) && name.ends_with(".db") {
            kept.push(entry.path());
        }
    }
    kept.sort();
    for stale in kept.iter().take(kept.len().saturating_sub(BACKUP_KEEP)) {
        if let Err(err) = tokio::fs::remove_file(stale).await {
            tracing::warn!(backup = %stale.display(), %err, "cannot prune old backup");
        }
    }
    Ok(())
}
