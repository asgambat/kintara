use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::auth;
use crate::error::AppError;
use crate::state::AppState;

/// The user a request acts as.
///
/// Resolved from the session cookie in OAuth mode. In `single-user` mode
/// every request acts as the shared local owner without a session; in
/// `proxy-header` mode the trusted reverse-proxy username header selects the
/// account (auto-provisioned on first sight).
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser(pub i64);

/// A real signed-in user used to make the stricter requirement explicit on
/// paid AI calls and account administration routes.
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedUser(pub i64);

async fn resolve_user_id(parts: &Parts, state: &AppState) -> Result<i64, AppError> {
    use crate::config::AuthMode;
    match state.config.auth_mode {
        AuthMode::SingleUser => auth::ensure_no_auth_owner(state).await,
        AuthMode::ProxyHeader => {
            let header_name = state.config.proxy_user_header.as_deref().ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("proxy user header not configured"))
            })?;
            let raw = parts
                .headers
                .get(header_name)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::Unauthorized("missing trusted proxy user".into()))?;
            auth::resolve_proxy_user(state, raw).await
        }
        AuthMode::Oauth => {
            let session = parts
                .headers
                .get(axum::http::header::COOKIE)
                .and_then(|value| value.to_str().ok())
                .and_then(auth::session_from_cookies)
                .ok_or_else(|| AppError::Unauthorized("sign in to continue".into()))?;
            auth::user_for_session(state, &session)
                .await?
                .ok_or_else(|| AppError::Unauthorized("sign in to continue".into()))
        }
    }
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(resolve_user_id(parts, state).await?))
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(CurrentUser(resolve_user_id(parts, state).await?))
    }
}
