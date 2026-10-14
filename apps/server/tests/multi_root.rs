//! Multiple library roots: the scanner indexes every configured root, identity
//! is (root_id, relative_path), and removing a file forgets only that entry.

use std::net::SocketAddr;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kintara_server::config::Config;
use kintara_server::state::AppState;
use kintara_server::{auth, db, routes, scanner};
use tower::ServiceExt;

struct RootsApp {
    router: axum::Router,
    db: sqlx::SqlitePool,
    primary: std::path::PathBuf,
    extra: std::path::PathBuf,
    cookie: String,
    _dir: tempfile::TempDir,
}

impl RootsApp {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let primary = dir.path().join("library");
        let extra = dir.path().join("more-books");

        let config = Config {
            library_dir: primary.clone(),
            extra_library_dirs: vec![extra.clone()],
            data_dir: dir.path().to_path_buf(),
            web_dir: dir.path().join("web"),
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            scan_on_start: false,
            watch: false,
            max_upload_bytes: 64 * 1024 * 1024,
            github_oauth: None,
            auth_mode: kintara_server::config::AuthMode::Oauth,
            local_user: "local".to_string(),
            proxy_user_header: None,
            proxy_admin_users: Vec::new(),
        };
        config.ensure_dirs().expect("create dirs");
        std::fs::create_dir_all(&config.web_dir).expect("create web dir");
        std::fs::write(config.web_dir.join("index.html"), "<!doctype html>spa")
            .expect("write index.html");

        let pool = db::connect(&config.database_path()).await.expect("db");
        let state = AppState::new(pool.clone(), config);
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users ORDER BY id LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("seeded user");
        let session = auth::create_session(&state, user_id)
            .await
            .expect("test session");
        let router = routes::router(state.clone());

        std::fs::write(primary.join("a.pdf"), b"%PDF-1.4 primary").unwrap();
        std::fs::write(extra.join("b.pdf"), b"%PDF-1.4 extra").unwrap();
        scanner::full_scan(&state).await.expect("scan");

        Self {
            router,
            db: pool,
            primary,
            extra,
            cookie: format!("kintara_session={session}"),
            _dir: dir,
        }
    }

    async fn get_documents(&self) -> serde_json::Value {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/documents")
                    .header("Cookie", self.cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).expect("json body")
    }
}

#[tokio::test]
async fn each_root_is_indexed_with_its_own_root_id() {
    let app = RootsApp::new().await;

    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT root_id, relative_path FROM documents ORDER BY root_id")
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![
            (0, "a.pdf".to_string()),
            (1, "b.pdf".to_string()),
        ]
    );

    // Same filename on both roots does not collide.
    std::fs::write(app.extra.join("a.pdf"), b"%PDF-1.4 same name").unwrap();
    let state = AppState::new(
        app.db.clone(),
        Config {
            library_dir: app.primary.clone(),
            extra_library_dirs: vec![app.extra.clone()],
            data_dir: app._dir.path().to_path_buf(),
            web_dir: app._dir.path().join("web"),
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            scan_on_start: false,
            watch: false,
            max_upload_bytes: 64 * 1024 * 1024,
            github_oauth: None,
            auth_mode: kintara_server::config::AuthMode::Oauth,
            local_user: "local".to_string(),
            proxy_user_header: None,
            proxy_admin_users: Vec::new(),
        },
    );
    scanner::full_scan(&state).await.expect("rescan");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE relative_path = 'a.pdf'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 2);

    let listed = app.get_documents().await;
    assert_eq!(listed["total"], 3);
}

#[tokio::test]
async fn removing_a_file_forgets_only_that_roots_entry() {
    let app = RootsApp::new().await;
    std::fs::remove_file(app.extra.join("b.pdf")).unwrap();

    let state = AppState::new(
        app.db.clone(),
        Config {
            library_dir: app.primary.clone(),
            extra_library_dirs: vec![app.extra.clone()],
            data_dir: app._dir.path().to_path_buf(),
            web_dir: app._dir.path().join("web"),
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            scan_on_start: false,
            watch: false,
            max_upload_bytes: 64 * 1024 * 1024,
            github_oauth: None,
            auth_mode: kintara_server::config::AuthMode::Oauth,
            local_user: "local".to_string(),
            proxy_user_header: None,
            proxy_admin_users: Vec::new(),
        },
    );
    scanner::full_scan(&state).await.expect("rescan");

    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT root_id, relative_path FROM documents")
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(rows, vec![(0, "a.pdf".to_string())]);
}
