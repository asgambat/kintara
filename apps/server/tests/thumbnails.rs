//! Background thumbnail generation (TODO T05).
//!
//! Real router, real SQLite, real poppler when it is installed: the acceptance
//! criteria are about timing and end state, so a mock would prove nothing.
//! When poppler is unavailable the timeouts part is skipped and the test only
//! asserts the upload path no longer waits for the thumbnail.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::{body_bytes, body_json, sample_pdf, TestApp};
use serde_json::Value;

/// Detects poppler once per binary; no poppler, no thumbnail work to verify.
async fn poppler_available() -> bool {
    tokio::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn an_upload_responses_without_waiting_for_the_thumbnail() {
    if !poppler_available().await {
        eprintln!("pdftoppm not available; only the non-blocking part is verified");
    }

    let app = TestApp::new().await;
    let response = app.upload("cover-me.pdf", &sample_pdf(), &[]).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let document = body_json(response).await;
    assert!(
        !document["hasThumbnail"].as_bool().unwrap(),
        "the upload must answer with the placeholder, not wait for the cover"
    );
}

#[tokio::test]
async fn the_worker_records_the_thumbnail_and_the_its_file_lives_in_the_data_dir() {
    if !poppler_available().await {
        eprintln!("pdftoppm not available; skipping the background generation check");
        return;
    }

    let app = TestApp::new().await;
    app.upload("worker-cover.pdf", &sample_pdf(), &[]).await;

    // One worker at a time drains the queue; polling covers scheduling jitter.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let thumbnail_name: Option<String> = loop {
        let name: Option<String> =
            sqlx::query_scalar("SELECT thumbnail_name FROM documents ORDER BY id LIMIT 1")
                .fetch_one(&app.db)
                .await
                .unwrap();
        if name.is_some() || tokio::time::Instant::now() > deadline {
            break name;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    };

    let name = thumbnail_name.expect("the worker must record the thumbnail in time");
    let on_disk = app.config.thumbnail_dir().join(&name);
    assert!(
        std::fs::metadata(&on_disk).is_ok(),
        "the recorded thumbnail must exist on disk"
    );

    // The public route now serves the generated file.
    let document_id: i64 =
        sqlx::query_scalar("SELECT id FROM documents ORDER BY id LIMIT 1")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let served = app
        .get(&format!("/api/documents/{document_id}/thumbnail"))
        .await;
    assert_eq!(served.status(), axum::http::StatusCode::OK);
    assert!(!body_bytes(served).await.is_empty());
}

/// A worker failure must not leave orphan files or a phantom cover: the entry
/// stays without a thumbnail and the placeholder keeps showing.
#[tokio::test]
async fn a_failed_thumbnail_job_leaves_no_orphan_and_no_cover() {
    // Any non-PDF payload makes pdftoppm fail deterministically even when
    // poppler is installed; when it is not, the path is skipped as everywhere
    // else in this file.
    if !poppler_available().await {
        eprintln!("pdftoppm not available; the failure path cannot be exercised");
        return;
    }

    let app = TestApp::new().await;
    // Attention: content that is syntactically a PDF for the extension check,
    // but garbage for poppler.
    let bogus = b"%PDF-1.4 broken no way poppler reads this without failing";
    let response = app.upload("broken-cover.pdf", bogus, &[]).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let document: Value = body_json(response).await;
    let document_id = document["id"].as_i64().unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let name: Option<String> =
            sqlx::query_scalar("SELECT thumbnail_name FROM documents WHERE id = ?")
                .bind(document_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(
            name.is_none(),
            "a failed generation must not be recorded as a cover"
        );
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let orphans = std::fs::read_dir(app.config.thumbnail_dir())
        .expect("list thumbnails")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("doc-{document_id}-"))
        })
        .count();
    assert_eq!(
        orphans, 0,
        "failed runs must clean up their partial files"
    );
}
