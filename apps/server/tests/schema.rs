//! Schema behaviour tests.
//!
//! These run against a real SQLite file created by the real migrations — no
//! mocks — because the things worth testing here (cascade deletes, FTS trigger
//! sync, CHECK constraints) are behaviours of SQLite itself, and a fake would
//! prove nothing.

use kintara_server::db;
use sqlx::Row;
use sqlx::SqlitePool;

async fn fresh_db() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = db::connect(&dir.path().join("kintara.db"))
        .await
        .expect("migrations should apply to an empty database");
    (dir, pool)
}

/// Inserts one user and one document, returning their ids. Ids are read back
/// rather than assumed, because AUTOINCREMENT does not restart at 1.
async fn seed(pool: &SqlitePool) -> (i64, i64) {
    let user_id: i64 = sqlx::query_scalar("INSERT INTO users (username) VALUES (?) RETURNING id")
        .bind("jess")
        .fetch_one(pool)
        .await
        .expect("insert user");

    let doc_id: i64 = sqlx::query_scalar(
        "INSERT INTO documents (owner_id, title, relative_path, document_type)
         VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind("Attention Is All You Need")
    .bind("papers/attention.pdf")
    .bind("pdf")
    .fetch_one(pool)
    .await
    .expect("insert document");

    (user_id, doc_id)
}

#[tokio::test]
async fn migrations_apply_to_an_empty_database() {
    let (_dir, pool) = fresh_db().await;

    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("list tables");

    for expected in [
        "annotations",
        "ai_conversations",
        "ai_message_citations",
        "ai_messages",
        "ai_usage",
        "collections",
        "documents",
        "document_pages",
        "github_invitations",
        "libraries",
        "library_members",
        "user_ai_settings",
        "user_document_state",
        "users",
    ] {
        assert!(
            tables.iter().any(|t| t == expected),
            "expected table {expected} to exist, got {tables:?}"
        );
    }
}

#[tokio::test]
async fn ai_conversations_are_per_user_and_cascade_with_the_document() {
    let (_dir, pool) = fresh_db().await;
    let (user_id, doc_id) = seed(&pool).await;
    let conversation_id: i64 = sqlx::query_scalar(
        "INSERT INTO ai_conversations (user_id, document_id) VALUES (?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let message_id: i64 = sqlx::query_scalar(
        "INSERT INTO ai_messages (conversation_id, role, kind, content)
         VALUES (?, 'assistant', 'question', 'Answer') RETURNING id",
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_message_citations (message_id, page_number, excerpt)
         VALUES (?, 1, 'Source')",
    )
    .bind(message_id)
    .execute(&pool)
    .await
    .unwrap();

    let duplicate =
        sqlx::query("INSERT INTO ai_conversations (user_id, document_id) VALUES (?, ?)")
            .bind(user_id)
            .bind(doc_id)
            .execute(&pool)
            .await;
    assert!(
        duplicate.is_err(),
        "a user has one conversation per document"
    );

    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .unwrap();
    let conversations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    let citations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_message_citations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((conversations, messages, citations), (0, 0, 0));
}

#[tokio::test]
async fn document_pages_are_numbered_and_cascade_with_the_document() {
    let (_dir, pool) = fresh_db().await;
    let (_user_id, doc_id) = seed(&pool).await;

    sqlx::query(
        "INSERT INTO document_pages (document_id, page_number, text) VALUES (?, 1, 'Body text')",
    )
    .bind(doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let zero_page = sqlx::query(
        "INSERT INTO document_pages (document_id, page_number, text) VALUES (?, 0, 'bad')",
    )
    .bind(doc_id)
    .execute(&pool)
    .await;
    assert!(zero_page.is_err(), "page numbers are one based");

    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_pages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn wal_and_foreign_keys_are_enabled_on_pool_connections() {
    let (_dir, pool) = fresh_db().await;

    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .expect("journal_mode");
    assert_eq!(journal.to_lowercase(), "wal");

    // Enforcement is per-connection in SQLite, so this has to be checked on a
    // pool connection rather than trusted from the migration file.
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .expect("foreign_keys");
    assert_eq!(fk, 1, "foreign key enforcement must be on");
}

#[tokio::test]
async fn deleting_a_document_cascades_to_per_user_state_and_annotations() {
    let (_dir, pool) = fresh_db().await;
    let (user_id, doc_id) = seed(&pool).await;

    sqlx::query(
        "INSERT INTO user_document_state (user_id, document_id, reading_progress, is_favorite)
         VALUES (?, ?, 0.42, 1)",
    )
    .bind(user_id)
    .bind(doc_id)
    .execute(&pool)
    .await
    .expect("insert reading state");

    sqlx::query(
        "INSERT INTO annotations (document_id, user_id, annotation_type, serialized_position)
         VALUES (?, ?, 'highlight', '{}')",
    )
    .bind(doc_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert annotation");

    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .expect("delete document");

    let state: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_document_state")
        .fetch_one(&pool)
        .await
        .unwrap();
    let annotations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(state, 0, "reading state should not outlive its document");
    assert_eq!(
        annotations, 0,
        "annotations should not outlive their document"
    );
}

#[tokio::test]
async fn deleting_a_library_cascades_to_its_collections() {
    let (_dir, pool) = fresh_db().await;

    let owner_id: i64 = sqlx::query_scalar("SELECT id FROM users ORDER BY id LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("seeded owner");

    let library_id: i64 = sqlx::query_scalar(
        "INSERT INTO libraries (owner_id, name) VALUES (?, 'Papers') RETURNING id",
    )
            .bind(owner_id)
            .fetch_one(&pool)
            .await
            .expect("insert library");

    sqlx::query("INSERT INTO collections (library_id, name) VALUES (?, '2026')")
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("insert collection");

    sqlx::query("DELETE FROM libraries WHERE id = ?")
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("delete library");

    let collections: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM collections")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(collections, 0);
}

#[tokio::test]
async fn annotation_type_is_constrained() {
    let (_dir, pool) = fresh_db().await;
    let (user_id, doc_id) = seed(&pool).await;

    let result = sqlx::query(
        "INSERT INTO annotations (document_id, user_id, annotation_type, serialized_position)
         VALUES (?, ?, 'scribble', '{}')",
    )
    .bind(doc_id)
    .bind(user_id)
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "annotation_type outside ('highlight','note') must be rejected"
    );
}

/// The shared-library access filter probes `library_documents` from the
/// document side once per candidate row (the reverse of the primary key), so
/// the migration must add the covering indexes and the planner must actually
/// reach for them, or a large library pays a scan per filtered listing page.
#[tokio::test]
async fn access_filters_resolve_through_reverse_indexes() {
    let (_dir, pool) = fresh_db().await;
    seed(&pool).await;

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND name IN
         ('idx_library_documents_document', 'idx_document_tags_tag')",
    )
    .fetch_all(&pool)
    .await
    .expect("read indexes");

    assert_eq!(
        indexes.len(),
        2,
        "both reverse indexes must be created, got {indexes:?}"
    );

    // Mirrors the shape `push_filters` builds, with the per-row access probe.
    let plans: Vec<String> = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT d.id FROM documents d WHERE (d.owner_id = 1 OR EXISTS (
            SELECT 1 FROM library_documents access_ld
            WHERE access_ld.document_id = d.id))
             AND d.id IN (SELECT document_id FROM document_tags WHERE tag_id = 1)",
    )
    .fetch_all(&pool)
    .await
    .expect("explain")
    .iter()
    .map(|row| row.get::<String, _>("detail"))
    .collect();

    assert!(
        plans
            .iter()
            .any(|detail| detail.contains("idx_library_documents_document")),
        "the access probe must use the reverse index, plan: {plans:?}"
    );
    assert!(
        plans.iter().any(|detail| detail.contains("idx_document_tags_tag")),
        "the tag filter must use the reverse index, plan: {plans:?}"
    );
}

/// auto_vacuum became incremental for fresh databases, so long installs
/// reclaim pages without ever requiring a blocking full VACUUM.
#[tokio::test]
async fn a_fresh_database_uses_incremental_auto_vacuum() {
    let (_dir, pool) = fresh_db().await;
    let mode: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&pool)
        .await
        .expect("auto_vacuum");
    assert_eq!(mode, 2, "2 is SQLite's incremental auto_vacuum");
}

/// Scheduled snapshots: `VACUUM INTO` writes a standalone file every time and
/// the rotation keeps only the newest five, wherever they came from.
#[tokio::test]
async fn snapshots_are_written_and_rotated() {
    use std::time::Duration;

    // Mirrors db::BACKUP_KEEP, which is private for a reason.
    const BACKUP_KEEP_TEST: usize = 5;

    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("kintara.db");
    let pool = db::connect(&db_path).await.expect("migrations");

    for _ in 0..(BACKUP_KEEP_TEST + 1) {
        db::backup(&pool, &db_path).await.expect("snapshot");
        // Snapshot names carry millisecond stamps, but the breather keeps the
        // ordering deterministic even on fast disks.
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    let backups = dir.path().join("backups");
    let kept: Vec<String> = std::fs::read_dir(&backups)
        .expect("list backups")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        kept.len(),
        BACKUP_KEEP_TEST,
        "rotation keeps only the newest snapshots, got {kept:?}"
    );

    // The snapshot file is a complete, standalone SQLite database — restore
    // is a file swap, so it must open even without the original around.
    let newest = std::fs::read_dir(&backups)
        .expect("list backups")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .next()
        .expect("snapshot exists");
    let snapshot: sqlx::SqlitePool = sqlx::pool::PoolOptions::new()
        .connect(&format!("sqlite://{}", newest.display()))
        .await
        .expect("snapshot opens as a database");
    let tables: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'")
            .fetch_one(&snapshot)
            .await
            .expect("read tables");
    assert!(tables > 0, "snapshot carries the schema");
}

#[tokio::test]
async fn fts_index_tracks_inserts_updates_and_deletes() {
    let (_dir, pool) = fresh_db().await;
    let (_user_id, doc_id) = seed(&pool).await;

    async fn matches(pool: &SqlitePool, term: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM documents_fts WHERE documents_fts MATCH ?")
            .bind(term)
            .fetch_one(pool)
            .await
            .expect("fts query")
    }

    assert_eq!(matches(&pool, "attention").await, 1, "insert should index");

    sqlx::query("UPDATE documents SET title = 'Sparse Attention 2017' WHERE id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .expect("update document");

    assert_eq!(matches(&pool, "2017").await, 1, "update should reindex");
    assert_eq!(
        matches(&pool, "\"Is All You Need\"").await,
        0,
        "the old title should no longer match after an update"
    );

    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(doc_id)
        .execute(&pool)
        .await
        .expect("delete document");

    assert_eq!(
        matches(&pool, "attention").await,
        0,
        "delete should deindex"
    );

    let fts_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents_fts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fts_rows, 0, "fts index must not drift from documents");
}

#[tokio::test]
async fn relative_path_is_unique() {
    let (_dir, pool) = fresh_db().await;
    seed(&pool).await;

    let duplicate = sqlx::query(
        "INSERT INTO documents (owner_id, title, relative_path, document_type)
         VALUES ((SELECT id FROM users ORDER BY id LIMIT 1), 'Another',
                 'papers/attention.pdf', 'pdf')",
    )
    .execute(&pool)
    .await;

    assert!(
        duplicate.is_err(),
        "the same relative path must not be indexed twice"
    );
}
