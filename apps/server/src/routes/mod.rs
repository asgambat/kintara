pub mod ai;
pub mod ai_chat;
pub mod ai_cover;
pub mod ai_find;
pub mod ai_metadata;
pub mod ai_search;
pub mod ai_settings;
pub mod annotations;
pub mod auth;
pub mod collections;
pub mod documents;
pub mod health;
pub mod libraries;
pub mod tags;
pub mod users;

use std::time::Duration;

use axum::Router;
use axum::http::HeaderValue;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use tower::Layer;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::error::AppError;
use crate::state::AppState;

/// Unmatched paths under `/api` must 404 as JSON. Without this they fall
/// through to the SPA fallback below and return index.html with a 200, which
/// turns every client fetch bug into a confusing HTML-parsed-as-JSON error.
async fn api_not_found() -> AppError {
    AppError::NotFound
}

/// Ceiling for JSON API calls. Streaming document bytes and uploads are
/// deliberately excluded (see `documents::router`): a slow link legitimately
/// takes longer than this. AI calls are excluded too — the inner reqwest
/// clients already bound them (90 s text, 180 s images), and a shorter route
/// timeout here would break image generation.
fn api_timeout() -> TimeoutLayer {
    TimeoutLayer::new(Duration::from_secs(120))
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health::health))
        .nest("/auth", auth::router().layer(api_timeout()))
        .route("/me", get(auth::me).route_layer(api_timeout()))
        .nest(
            "/ai",
            ai::router()
                .merge(ai_chat::router())
                .merge(ai_cover::router())
                .merge(ai_find::router())
                .merge(ai_metadata::router())
                .merge(ai_search::router())
                .merge(ai_settings::router()),
        )
        .nest(
            "/documents",
            documents::router(state.config.max_upload_bytes),
        )
        .nest("/libraries", libraries::router().layer(api_timeout()))
        .nest("/collections", collections::router().layer(api_timeout()))
        .nest("/tags", tags::router().layer(api_timeout()))
        .route("/users", get(users::list_access).route_layer(api_timeout()))
        .nest("/users", users::router().layer(api_timeout()))
        .route("/annotations", post(annotations::create).route_layer(api_timeout()))
        .route(
            "/annotations/{id}",
            axum::routing::delete(annotations::delete).route_layer(api_timeout()),
        )
        .fallback(api_not_found)
        .with_state(state.clone());

    // The frontend is a single-page app, so any unmatched path that is not a
    // real file has to return index.html and let the client router handle it.
    let index = state.config.web_dir.join("index.html");
    let spa = ServeDir::new(&state.config.web_dir).fallback(ServeFile::new(index));

    // Compression is attached to the static bundle here, and to the JSON
    // document routes inside `documents::router`. It is deliberately never in
    // front of document bytes: PDFs are already compressed, gzip would burn NAS
    // CPU for nothing, and compressing a 206 range response is simply wrong.
    let spa = CompressionLayer::new().layer(spa);

    Router::new()
        .nest("/api", api)
        .fallback_service(spa)
        .layer(middleware::from_fn(immutable_assets))
        .layer(TraceLayer::new_for_http())
}

/// Long-lived cache for fingerprinted bundles. Vite emits JS/CSS with a content
/// hash (`app-Ab12Cd34.js`), so a year-long `immutable` is always correct for
/// them. The brand aliases under `assets/brand/` are the exception — same name,
/// changing bytes — and they fail the hash check below, as does everything
/// outside `/assets/` (`index.html`, `/api/*`). Only successful responses are
/// marked; a 404 for a stale hashed name must stay revalidatable.
async fn immutable_assets(req: axum::extract::Request, next: Next) -> Response {
    let fingerprint = is_hashed_asset(req.uri().path());
    let mut response = next.run(req).await;
    if fingerprint && response.status().is_success() {
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}

fn is_hashed_asset(path: &str) -> bool {
    let Some(file) = path.strip_prefix("/assets/") else {
        return false;
    };
    let file = file.rsplit('/').next().unwrap_or(file);
    let stem = file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file);
    stem.rsplit_once('-').is_some_and(|(_, hash)| {
        hash.len() >= 8
            && hash
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}
