use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::config::Config;

/// Shared handler state. Cloning is cheap — the pool is internally reference
/// counted and the config sits behind an `Arc`.
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    /// Sender of the background thumbnail queue (T05). Upload and scan enqueue
    /// here and answer immediately; the dedicated worker generates the JPEG
    /// and records it, so a heavy first page never holds a request open.
    pub thumbnail_jobs: mpsc::UnboundedSender<ThumbnailJob>,
}

/// One queued thumbnail generation. `previous` is the filename currently
/// recorded in `documents.thumbnail_name`, if any — the worker deletes it only
/// after a replacement is safely recorded (the refresh path through a changed
/// file), and `None` means "this document must not have a cover yet".
#[derive(Debug)]
pub struct ThumbnailJob {
    pub document_id: i64,
    pub source: PathBuf,
    pub previous: Option<String>,
}

/// Total attempts per job. Limited on purpose: a poppler binary that is
/// genuinely missing or a PDF that deterministically crashes it would otherwise
/// be retried forever, while one extra attempt covers a transient NAS hiccup.
const THUMBNAIL_ATTEMPTS: usize = 2;

impl AppState {
    pub fn new(db: SqlitePool, config: Config) -> Self {
        // Unbounded rather than bounded: the queue holds only path triples, and
        // a bounded queue would make the scan and even an upload wait on a
        // worker behind thousands of backlogged covers from a first sweep.
        let (thumbnail_jobs, thumbnail_rx) = mpsc::unbounded_channel();
        let state = Self {
            db,
            config: Arc::new(config),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .expect("valid HTTP client configuration"),
            thumbnail_jobs,
        };

        // One dedicated worker. Spawned from `new` so every state built — main,
        // tests — gets a live consumer without main having to remember.
        tokio::spawn(thumbnail_worker(state.clone(), thumbnail_rx));
        state
    }

    /// Queues a thumbnail generation. Never blocks the caller; a lost job
    /// (worker crashed) costs at most a missing cover, never an error.
    pub fn enqueue_thumbnail(&self, job: ThumbnailJob) {
        if let Err(err) = self.thumbnail_jobs.send(job) {
            tracing::warn!(%err, "thumbnail worker unavailable; skipping cover");
        }
    }
}

/// Consumes the thumbnail queue for the whole process life.
async fn thumbnail_worker(state: AppState, mut rx: mpsc::UnboundedReceiver<ThumbnailJob>) {
    while let Some(job) = rx.recv().await {
        process_thumbnail_job(&state, &job).await;
    }
}

async fn process_thumbnail_job(state: &AppState, job: &ThumbnailJob) {
    let thumbnail_dir = state.config.thumbnail_dir();

    for attempt in 1..=THUMBNAIL_ATTEMPTS {
        let Some(name) = crate::media::generate_thumbnail(
            &job.source,
            &thumbnail_dir,
            job.document_id,
            state.config.poppler_timeout,
        )
        .await
        else {
            // Generation failed (missing poppler, broken PDF, timeout). The
            // file cleanup is `generate_thumbnail`'s own job, so nothing here:
            // stop rather than retry what will deterministically fail again.
            if attempt == THUMBNAIL_ATTEMPTS {
                tracing::warn!(
                    document_id = job.document_id,
                    "thumbnail generation failed; leaving the document without a cover"
                );
            }
            continue;
        };

        // Guard the race against an uploaded custom cover: the update only
        // lands when the recorded name is still the one this job started from
        // (`IS ?` treats NULL correctly for a fresh upload). A newer cover
        // wins and the freshly rendered JPEG is disposed of.
        let recorded = sqlx::query(
            "UPDATE documents\n             SET thumbnail_name = ?\n             WHERE id = ? AND thumbnail_name IS ?",
        )
        .bind(&name)
        .bind(job.document_id)
        .bind(&job.previous)
        .execute(&state.db)
        .await;

        match recorded {
            Ok(result) => {
                if result.rows_affected() > 0 {
                    if let Some(old) = &job.previous
                        && let Ok(old_path) = crate::files::resolve_in_root(&thumbnail_dir, old)
                        && old_path != thumbnail_dir.join(&name)
                    {
                        let _ = tokio::fs::remove_file(old_path).await;
                    }
                } else {
                    // Nothing points at the freshly written file: remove it so
                    // the newer cover the user already put in place survives.
                    let _ = tokio::fs::remove_file(thumbnail_dir.join(&name)).await;
                    tracing::debug!(
                        document_id = job.document_id,
                        "generated thumbnail is stale; a newer cover was recorded meanwhile"
                    );
                }
                return;
            }
            Err(update_err) => {
                // Orphan guard: no (correct) row points at the just-written
                // file, so it must not survive in thumbnail_dir.
                let _ = tokio::fs::remove_file(thumbnail_dir.join(&name)).await;
                if attempt == THUMBNAIL_ATTEMPTS {
                    tracing::warn!(
                        document_id = job.document_id,
                        err = %update_err,
                        "could not record the generated thumbnail; file removed"
                    );
                }
            }
        }
    }
}
