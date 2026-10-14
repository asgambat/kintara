//! Homelab auth modes: `single-user` (no login) and `proxy-header` (trusted
//! reverse-proxy username header with one auto-provisioned account per value).

use std::net::SocketAddr;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kintara_server::auth;
use kintara_server::config::{AuthMode, Config};
use kintara_server::state::AppState;
use kintara_server::{db, routes};
use tower::ServiceExt;

struct NoAuthApp {
    router: axum::Router,
    db: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
}

impl NoAuthApp {
    async fn with_config(mut make_config: impl FnMut(&tempfile::TempDir) -> Config) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = make_config(&dir);
        config.ensure_dirs().expect("create dirs");
        std::fs::create_dir_all(&config.web_dir).expect("create web dir");
        std::fs::write(config.web_dir.join("index.html"), "<!doctype html>spa")
            .expect("write index.html");
        let pool = db::connect(&config.database_path()).await.expect("db");
        let router = routes::router(AppState::new(pool.clone(), config));
        Self {
            router,
            db: pool,
            _dir: dir,
        }
    }

    async fn single_user() -> Self {
        Self::with_config(|dir| Config {
            library_dir: dir.path().join("library"),
            data_dir: dir.path().to_path_buf(),
            web_dir: dir.path().join("web"),
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            scan_on_start: false,
            watch: false,
            scan_jobs: 2,
            max_upload_bytes: 64 * 1024 * 1024,
            extra_library_dirs: Vec::new(),
            github_oauth: None,
            auth_mode: AuthMode::SingleUser,
            local_user: "homelab".to_string(),
            proxy_user_header: None,
            proxy_admin_users: Vec::new(),
        })
        .await
    }

    async fn proxy_header() -> Self {
        Self::with_config(|dir| Config {
            library_dir: dir.path().join("library"),
            data_dir: dir.path().to_path_buf(),
            web_dir: dir.path().join("web"),
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            scan_on_start: false,
            watch: false,
            scan_jobs: 2,
            max_upload_bytes: 64 * 1024 * 1024,
            extra_library_dirs: Vec::new(),
            github_oauth: None,
            auth_mode: AuthMode::ProxyHeader,
            local_user: "local".to_string(),
            proxy_user_header: Some("x-test-user".to_string()),
            proxy_admin_users: Vec::new(),
        })
        .await
    }

    async fn request(&self, request: Request<Body>) -> axum::response::Response {
        self.router.clone().oneshot(request).await.unwrap()
    }

    async fn get(&self, uri: &str, headers: &[(&str, &str)]) -> axum::response::Response {
        let mut builder = Request::builder().uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.request(builder.body(Body::empty()).unwrap()).await
    }
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

#[test]
fn proxy_usernames_are_trimmed_and_restricted() {
    assert_eq!(
        auth::normalize_proxy_username("  Alice-1 ").as_deref(),
        Some("Alice-1")
    );
    assert_eq!(
        auth::normalize_proxy_username("user@example.com").as_deref(),
        Some("user@example.com")
    );
    assert!(auth::normalize_proxy_username("").is_none());
    assert!(auth::normalize_proxy_username("   ").is_none());
    assert!(auth::normalize_proxy_username("a/b").is_none());
    assert!(auth::normalize_proxy_username("a;b").is_none());
    assert!(auth::normalize_proxy_username(&"a".repeat(65)).is_none());
}

#[tokio::test]
async fn single_user_reports_authenticated_without_a_session() {
    let app = NoAuthApp::single_user().await;
    let status = body_json(app.get("/api/auth/status", &[]).await).await;
    assert_eq!(status["authMode"], "single-user");
    assert_eq!(status["authenticated"], true);
    assert_eq!(status["needsOwner"], false);
    assert_eq!(status["oauthConfigured"], false);
    assert_eq!(status["user"]["username"], "homelab");
    assert_eq!(status["user"]["isAdmin"], true);
}

#[tokio::test]
async fn single_user_serves_the_api_without_a_cookie() {
    let app = NoAuthApp::single_user().await;
    let response = app.get("/api/documents", &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn single_user_disables_the_github_flow() {
    let app = NoAuthApp::single_user().await;
    assert_eq!(
        app.get("/api/auth/github/start", &[]).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        app.get("/api/auth/github/callback?code=x&state=y", &[])
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    // Logout is a no-op but stays 204 so shared clients do not error.
    let response = app
        .request(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn single_user_reuses_the_seeded_owner_row() {
    let app = NoAuthApp::single_user().await;
    let status = body_json(app.get("/api/auth/status", &[]).await).await;
    assert_eq!(status["user"]["username"], "homelab");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn proxy_header_needs_its_header_on_status_and_api() {
    let app = NoAuthApp::proxy_header().await;
    let anonymous = body_json(app.get("/api/auth/status", &[]).await).await;
    assert_eq!(anonymous["authMode"], "proxy-header");
    assert_eq!(anonymous["authenticated"], false);
    assert_eq!(app.get("/api/documents", &[]).await.status(), StatusCode::UNAUTHORIZED);

    let alice = body_json(
        app.get("/api/auth/status", &[("x-test-user", "alice")]).await,
    )
    .await;
    assert_eq!(alice["authenticated"], true);
    assert_eq!(alice["user"]["username"], "alice");
    // First human account becomes admin so a fresh homelab has an owner.
    assert_eq!(alice["user"]["isAdmin"], true);
    assert_eq!(
        app.get("/api/documents", &[("x-test-user", "alice")])
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn proxy_header_provisions_one_account_per_value() {
    let app = NoAuthApp::proxy_header().await;
    for user in ["alice", "bob"] {
        let status = body_json(app.get("/api/auth/status", &[("x-test-user", user)]).await).await;
        assert_eq!(status["authenticated"], true);
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE username IN ('alice', 'bob')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 2);
    // Only the first human account is admin by default.
    let bob_admin: i64 =
        sqlx::query_scalar("SELECT is_admin FROM users WHERE username = 'bob'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(bob_admin, 0);
}

#[tokio::test]
async fn proxy_header_rejects_invalid_usernames() {
    let app = NoAuthApp::proxy_header().await;
    assert_eq!(
        app.get("/api/documents", &[("x-test-user", "a/b")])
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.get("/api/documents", &[("x-test-user", "   ")])
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn proxy_header_disables_github_and_invitations() {
    let app = NoAuthApp::proxy_header().await;
    assert_eq!(
        app.get("/api/auth/github/start", &[]).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let response = app
        .request(
            Request::builder()
                .method("POST")
                .uri("/api/users/invitations")
                .header("content-type", "application/json")
                .header("x-test-user", "alice")
                .body(Body::from(r#"{"githubLogin":"bob"}"#))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
