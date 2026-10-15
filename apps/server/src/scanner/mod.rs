pub mod index;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use walkdir::WalkDir;

use crate::error::AppResult;
use crate::state::AppState;

pub use index::{is_indexable, Outcome};

/// Walks the library and brings the database in line with what is on disk.
///
/// Runs at startup because files arrive over SMB while the server is down, and
/// because the watcher only ever reports changes it was running for.
pub async fn full_scan(state: &AppState) -> AppResult<()> {
    let roots = state.config.all_library_roots();
    tracing::info!(roots = roots.len(), "scanning libraries");

    let mut seen: HashSet<(i64, String)> = HashSet::new();
    let mut added = 0usize;
    let mut updated = 0usize;
    let mut scanned = 0usize;

    for (root_id, root) in &roots {
        let root_id = *root_id;
        tracing::info!(root_id, root = %root.display(), "scanning library");
        let walk_root = root.clone();
        let files: Vec<PathBuf> = tokio::task::spawn_blocking(move || {
            WalkDir::new(&walk_root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| entry.into_path())
                .filter(|path| index::is_indexable(path))
                // Skip @eaDir, #recycle and friends, which NAS software keeps
                // beside your files and which would otherwise fill the library.
                .filter(|path| {
                    index::relative_to_root(&walk_root, path)
                        .map(|rel| !index::is_ignored_relative(&rel))
                        .unwrap_or(false)
                })
                .collect()
        })
        .await
        .map_err(|err| crate::error::AppError::Internal(err.into()))?;

        scanned += files.len();
        // Files are indexed concurrently, bounded by `scan_jobs`: each PDF
        // spawns up to three poppler processes, so an unbounded fan-out would
        // thrash a small NAS. The permit is acquired before spawning, which
        // also bounds how many tasks pile up waiting.
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(
            state.config.scan_jobs.max(1),
        ));
        let mut indexing: JoinSet<(PathBuf, AppResult<Outcome>)> = JoinSet::new();
        for path in files {
            if let Some((id, relative)) = index::root_id_for_path(&state.config, &path) {
                debug_assert_eq!(id, root_id);
                seen.insert((id, relative));
            }
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("scan semaphore is never closed");
            let task_state = state.clone();
            indexing.spawn(async move {
                let _permit = permit;
                let outcome = index::index_file(&task_state, &path).await;
                (path, outcome)
            });
        }
        while let Some(finished) = indexing.join_next().await {
            match finished {
                Ok((_, Ok(Outcome::Added(_)))) => added += 1,
                Ok((_, Ok(Outcome::Updated(_)))) => updated += 1,
                Ok((_, Ok(Outcome::Skipped))) => {}
                Ok((path, Err(err))) => {
                    tracing::warn!(path = %path.display(), ?err, "failed to index")
                }
                Err(err) => tracing::warn!(?err, "index task failed"),
            }
        }
    }

    // Anything indexed but no longer on disk was deleted while the server was
    // down, or straight off the share.
    let known: Vec<(i64, String)> =
        sqlx::query_as("SELECT root_id, relative_path FROM documents")
            .fetch_all(&state.db)
            .await?;

    // Rows whose root is not configured anymore (entry removed, share
    // unmounted) are left alone: the files may come back, and forgetting them
    // would silently wipe the library on a transient mount failure.
    let configured: HashSet<i64> = roots.iter().map(|(id, _)| *id).collect();
    let mut removed = 0usize;
    for (root_id, relative) in known {
        if !configured.contains(&root_id) {
            continue;
        }
        if !seen.contains(&(root_id, relative.clone()))
            && index::forget_path(state, root_id, &relative).await? {
                removed += 1;
            }
    }

    tracing::info!(
        scanned,
        added,
        updated,
        removed,
        "library scan complete"
    );
    Ok(())
}

/// True for a filesystem event the watcher should react to.
///
/// inotify (via `notify`) watches `OPEN | CLOSE_WRITE | ATTRIB` as well as
/// content changes, which surface as `EventKind::Access(..)` and
/// `EventKind::Modify(ModifyKind::Metadata(..))`. Indexing a file *opens* it
/// (hash read, `pdfinfo`, `pdftotext`, `pdftoppm`), so reacting to those
/// would re-queue the file we just indexed — a self-feeding loop that pins a
/// CPU with hashing and poppler work while logging nothing (unchanged hashes
/// resolve to `Outcome::Skipped`). Only Access and Metadata are dropped: every
/// other kind (create, remove, data/name/other modifies, overflow notices)
/// is treated as a potential content change, so no backend can go blind.
/// Deletions surface as `Remove` and renames as `Modify(Name(..))`.
fn is_actionable(event: &notify::Event) -> bool {
    use notify::event::{EventKind, ModifyKind};

    if event.need_rescan() {
        return true;
    }
    !matches!(
        event.kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_))
    )
}

/// Watches the library for changes and keeps the index current.
///
/// Events are debounced: a single copy over SMB produces a burst of writes, and
/// reacting to each one would hash and re-thumbnail a file that is still being
/// written. The watcher runs for the life of the process.
///
/// Only content lifecycle events are queued (see [`is_actionable`]): access
/// and metadata events are dropped at the notify callback, before they ever
/// reach the debounce queue, so indexing a file cannot re-trigger itself.
pub fn spawn_watcher(state: AppState) {
    let roots = state.config.all_library_roots();

    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::unbounded_channel::<notify::Event>();

        let mut watcher = match notify::recommended_watcher(move |result| {
            if let Ok(event) = result
                && is_actionable(&event)
            {
                // A closed receiver just means shutdown.
                let _ = tx.send(event);
            }
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                tracing::warn!(%err, "could not start the library watcher; \
                    new files will only appear after a restart");
                return;
            }
        };

        let mut watched = 0usize;
        for (_, root) in &roots {
            if let Err(err) = watcher.watch(root, RecursiveMode::Recursive) {
                tracing::warn!(%err, root = %root.display(), "could not watch the library");
            } else {
                watched += 1;
            }
        }
        if watched == 0 {
            tracing::warn!("could not watch any library; new files will only appear after a restart");
            return;
        }

        tracing::info!(watched, "watching libraries for changes");

        let mut pending: HashSet<PathBuf> = HashSet::new();
        let mut rescan_requested = false;

        loop {
            // Collect events until the share goes quiet, then act once.
            let event = match rx.recv().await {
                Some(event) => event,
                None => break,
            };
            if event.need_rescan() {
                rescan_requested = true;
            }
            pending.extend(event.paths);

            loop {
                match tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await {
                    Ok(Some(event)) => {
                        if event.need_rescan() {
                            rescan_requested = true;
                        }
                        pending.extend(event.paths);
                    }
                    Ok(None) => break,
                    Err(_) => break, // quiet period reached
                }
            }

            // An overflow (Flag::Rescan) means the kernel dropped events: some
            // change went unseen, so reconcile the whole tree rather than
            // trusting the partial pending set.
            if rescan_requested {
                rescan_requested = false;
                pending.clear();
                if let Err(err) = full_scan(&state).await {
                    tracing::warn!(?err, "rescan after watcher overflow failed");
                }
                continue;
            }

            for path in pending.drain() {
                if !index::is_indexable(&path) {
                    continue;
                }

                let located = index::root_id_for_path(&state.config, &path);
                if located
                    .as_ref()
                    .map(|(_, relative)| index::is_ignored_relative(relative))
                    .unwrap_or(true)
                {
                    continue;
                }

                if path.exists() {
                    if let Err(err) = index::index_file(&state, &path).await {
                        tracing::warn!(path = %path.display(), ?err, "failed to index");
                    }
                } else if let Some((root_id, relative)) = located
                    && let Err(err) = index::forget_path(&state, root_id, &relative).await {
                        tracing::warn!(%relative, ?err, "failed to remove");
                    }
            }
        }

        // Dropping the watcher here rather than earlier is what keeps it alive
        // for the life of the task.
        drop(watcher);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{
        AccessKind, AccessMode, DataChange, EventKind, MetadataKind, ModifyKind,
    };

    fn event(kind: EventKind) -> notify::Event {
        notify::Event::new(kind).add_path(PathBuf::from("/library/paper.pdf"))
    }

    #[test]
    fn open_and_close_events_do_not_requeue_the_file_just_indexed() {
        // Indexing a file opens and closes it; reacting to those would feed
        // the file straight back into the debounce queue, forever.
        assert!(!is_actionable(&event(EventKind::Access(AccessKind::Open(
            AccessMode::Any
        )))));
        assert!(!is_actionable(&event(EventKind::Access(AccessKind::Close(
            AccessMode::Read
        )))));
        assert!(!is_actionable(&event(EventKind::Access(AccessKind::Close(
            AccessMode::Write
        )))));
    }

    #[test]
    fn metadata_only_changes_do_not_requeue() {
        assert!(!is_actionable(&event(EventKind::Modify(
            ModifyKind::Metadata(MetadataKind::Any)
        ))));
    }

    #[test]
    fn content_lifecycle_events_still_reach_the_indexer() {
        use notify::event::{CreateKind, RemoveKind, RenameMode};

        assert!(is_actionable(&event(EventKind::Create(CreateKind::File))));
        assert!(is_actionable(&event(EventKind::Create(CreateKind::Folder))));
        assert!(is_actionable(&event(EventKind::Remove(RemoveKind::File))));
        assert!(is_actionable(&event(EventKind::Modify(
            ModifyKind::Data(DataChange::Content)
        ))));
        assert!(is_actionable(&event(EventKind::Modify(
            ModifyKind::Name(RenameMode::Both)
        ))));
        assert!(is_actionable(&event(EventKind::Modify(ModifyKind::Other))));
        assert!(is_actionable(&event(EventKind::Other)));
        assert!(is_actionable(&event(EventKind::Any)));
    }

    #[test]
    fn an_overflow_notice_requests_a_rescan() {
        let overflow = event(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert!(overflow.need_rescan());
        assert!(is_actionable(&overflow));
    }
}
