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

    let options = SqliteConnectOptions::from_str(&url)
        .with_context(|| format!("invalid database path {}", database_path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .context("failed to open the Kintara database")?;

    let migrator = sqlx::migrate!("./migrations");
    if has_pending(&pool, &migrator).await? {
        backup_before_migrate(database_path).await?;
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

/// Copies the database file aside before migrations run, so a failed or
/// surprising migration can be undone by stopping the server and swapping the
/// backup back. Keeps the newest [`BACKUP_KEEP`] copies and prunes the rest.
async fn backup_before_migrate(database_path: &Path) -> Result<()> {
    let meta = tokio::fs::metadata(database_path)
        .await
        .with_context(|| format!("cannot stat {}", database_path.display()))?;
    if !meta.is_file() || meta.len() == 0 {
        return Ok(());
    }

    let Some(data_dir) = database_path.parent() else {
        return Ok(());
    };
    let backups = data_dir.join("backups");
    tokio::fs::create_dir_all(&backups)
        .await
        .with_context(|| format!("cannot create {}", backups.display()))?;

    let stem = database_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("kintara");
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let dest = backups.join(format!("{stem}-{stamp}.db"));
    tokio::fs::copy(database_path, &dest)
        .await
        .with_context(|| format!("cannot back up to {}", dest.display()))?;
    tracing::info!(backup = %dest.display(), "database backed up before migrations");

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
