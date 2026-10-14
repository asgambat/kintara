use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct GitHubOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub public_url: String,
}

/// How the server authenticates browsers.
///
/// * `Oauth` is the default historical behaviour (GitHub OAuth + invitations).
/// * `SingleUser` disables login entirely: every request acts as one shared
///   local owner. Intended for a homelab LAN behind a reverse proxy that
///   already authenticates.
/// * `ProxyHeader` trusts a username header set by the reverse proxy
///   (Authelia / Authentik / Pangolin ...) and auto-provisions one account
///   per distinct header value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Oauth,
    SingleUser,
    ProxyHeader,
}

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Oauth => "oauth",
            Self::SingleUser => "single-user",
            Self::ProxyHeader => "proxy-header",
        }
    }

    pub fn is_no_auth(self) -> bool {
        !matches!(self, Self::Oauth)
    }
}

/// Runtime configuration, read entirely from the environment.
///
/// Defaults are relative paths so `cargo run` works from the crate directory
/// without setup. The container image overrides all of them with absolute
/// paths (`/library`, `/data`, `/app/web`).
#[derive(Debug, Clone)]
pub struct Config {
    /// Primary root of the document library. Every `documents.relative_path`
    /// with `root_id = 0` is resolved against this, which is what lets the
    /// volume move. Uploads always land here.
    pub library_dir: PathBuf,
    /// Additional read roots scanned and watched alongside the primary one.
    /// `documents.root_id` is the index into `[library_dir, ..extra_library_dirs]`.
    pub extra_library_dirs: Vec<PathBuf>,
    /// Holds the SQLite database and generated thumbnails. Kept separate from
    /// `library_dir` because that one is typically also a network share, and
    /// SQLite over SMB/NFS corrupts.
    pub data_dir: PathBuf,
    /// Directory containing the built frontend (`apps/web/dist`).
    pub web_dir: PathBuf,
    pub bind: SocketAddr,
    /// Sweep the library at startup. Files arrive over SMB while the server is
    /// down, so this is on by default.
    pub scan_on_start: bool,
    /// Watch the library for changes while running. Worth turning off on a
    /// share where inotify watches are scarce or the filesystem does not
    /// report events at all.
    pub watch: bool,
    /// Largest upload accepted, in bytes. Scanned books run to hundreds of
    /// megabytes, and axum's 2 MB default rejects almost any real PDF.
    pub max_upload_bytes: usize,
    /// OAuth credentials stay on the server; the frontend receives only a
    /// relative URL that begins the authorization flow.
    pub github_oauth: Option<GitHubOAuthConfig>,
    pub auth_mode: AuthMode,
    /// Username of the shared local owner used in `single-user` mode and as
    /// the fallback seed in `proxy-header` mode.
    pub local_user: String,
    /// Trusted username header for `proxy-header` mode, e.g.
    /// `X-Forwarded-User`. Lowercased at parse time.
    pub proxy_user_header: Option<String>,
    /// Usernames (lowercased) that are always admin in `proxy-header` mode.
    pub proxy_admin_users: Vec<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let library_dir = path_var("KINTARA_LIBRARY_DIR", "./data/library");
        let extra_library_dirs = extra_dirs_var("KINTARA_EXTRA_LIBRARY_DIRS");
        let data_dir = path_var("KINTARA_DATA_DIR", "./data");
        let web_dir = path_var("KINTARA_WEB_DIR", "../web/dist");

        let bind_raw = std::env::var("KINTARA_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let bind = bind_raw
            .parse::<SocketAddr>()
            .with_context(|| format!("KINTARA_BIND is not a valid address: {bind_raw}"))?;

        let auth_mode = auth_mode_from_env()?;
        let local_user = local_user_from_env()?;
        let proxy_user_header = proxy_user_header_from_env(auth_mode)?;
        let proxy_admin_users = proxy_admin_users_from_env();

        // In no-auth modes GitHub OAuth is ignored so a homelab does not need
        // a public URL or an OAuth app at all. Keeping the vars set is not an
        // error; they are just unused until the mode is switched back.
        let github_oauth = if auth_mode == AuthMode::Oauth {
            github_oauth_from_env()?
        } else {
            None
        };

        Ok(Self {
            library_dir,
            extra_library_dirs,
            data_dir,
            web_dir,
            bind,
            scan_on_start: bool_var("KINTARA_SCAN_ON_START", true),
            watch: bool_var("KINTARA_WATCH", true),
            max_upload_bytes: std::env::var("KINTARA_MAX_UPLOAD_MB")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(1024)
                .saturating_mul(1024 * 1024),
            github_oauth,
            auth_mode,
            local_user,
            proxy_user_header,
            proxy_admin_users,
        })
    }

    /// Creates the directories the server writes to. The library directory is
    /// included because a first run against an empty volume is normal.
    pub fn ensure_dirs(&self) -> Result<()> {
        let mut dirs = vec![&self.library_dir, &self.data_dir, &self.thumbnail_dir()];
        dirs.extend(self.extra_library_dirs.iter());
        for dir in dirs {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create directory {}", dir.display()))?;
        }
        Ok(())
    }

    /// Every scan root as `(root_id, path)`. Root 0 is always the primary
    /// `library_dir`; extras follow in configuration order.
    pub fn all_library_roots(&self) -> Vec<(i64, PathBuf)> {
        let mut roots = vec![(0, self.library_dir.clone())];
        roots.extend(
            self.extra_library_dirs
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, dir)| (i as i64 + 1, dir)),
        );
        roots
    }

    /// Filesystem root for a stored `documents.root_id`, or `None` when the
    /// configuration no longer contains that root (e.g. an env entry was
    /// removed). Callers treat that as not-found rather than an error.
    pub fn root_dir(&self, root_id: i64) -> Option<&std::path::Path> {
        if root_id == 0 {
            Some(&self.library_dir)
        } else {
            self.extra_library_dirs
                .get((root_id - 1) as usize)
                .map(PathBuf::as_path)
        }
    }

    pub fn thumbnail_dir(&self) -> PathBuf {
        self.data_dir.join("thumbnails")
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("kintara.db")
    }
}

fn github_oauth_from_env() -> Result<Option<GitHubOAuthConfig>> {
    let client_id = std::env::var("KINTARA_GITHUB_CLIENT_ID").ok();
    let client_secret = std::env::var("KINTARA_GITHUB_CLIENT_SECRET").ok();
    let public_url = std::env::var("KINTARA_PUBLIC_URL").ok();

    if client_id.is_none() && client_secret.is_none() && public_url.is_none() {
        return Ok(None);
    }

    let missing = [
        ("KINTARA_GITHUB_CLIENT_ID", client_id.is_none()),
        ("KINTARA_GITHUB_CLIENT_SECRET", client_secret.is_none()),
        ("KINTARA_PUBLIC_URL", public_url.is_none()),
    ]
    .into_iter()
    .filter_map(|(name, absent)| absent.then_some(name))
    .collect::<Vec<_>>();
    if !missing.is_empty() {
        anyhow::bail!(
            "GitHub login configuration is incomplete; missing {}",
            missing.join(", ")
        );
    }

    let public_url = public_url.unwrap().trim_end_matches('/').to_string();
    let parsed = url::Url::parse(&public_url)
        .with_context(|| "KINTARA_PUBLIC_URL must be an absolute URL")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("KINTARA_PUBLIC_URL must use http or https");
    }

    Ok(Some(GitHubOAuthConfig {
        client_id: client_id.unwrap(),
        client_secret: client_secret.unwrap(),
        public_url,
    }))
}

fn auth_mode_from_env() -> Result<AuthMode> {
    let raw = std::env::var("KINTARA_AUTH_MODE").unwrap_or_else(|_| "oauth".to_string());
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "oauth" | "github" => Ok(AuthMode::Oauth),
        "single-user" | "singleuser" | "disabled" | "no-auth" | "noauth" | "none" => {
            Ok(AuthMode::SingleUser)
        }
        "proxy-header" | "proxyheader" | "proxy" | "header" | "forwarded-user" => {
            Ok(AuthMode::ProxyHeader)
        }
        _ => anyhow::bail!(
            "KINTARA_AUTH_MODE must be one of oauth, single-user, proxy-header (got '{raw}')"
        ),
    }
}

fn local_user_from_env() -> Result<String> {
    let raw = std::env::var("KINTARA_LOCAL_USER").unwrap_or_else(|_| "local".to_string());
    let name = raw.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("KINTARA_LOCAL_USER cannot be empty");
    }
    if name.len() > 64 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' ) {
        anyhow::bail!(
            "KINTARA_LOCAL_USER must be 1-64 chars of letters, digits, '-', '_' or '.'"
        );
    }
    Ok(name)
}

fn proxy_user_header_from_env(mode: AuthMode) -> Result<Option<String>> {
    let raw = std::env::var("KINTARA_PROXY_USER_HEADER")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    match (mode, raw) {
        (AuthMode::ProxyHeader, None) => anyhow::bail!(
            "KINTARA_PROXY_USER_HEADER is required when KINTARA_AUTH_MODE=proxy-header"
        ),
        (AuthMode::ProxyHeader, Some(name)) => {
            let lower = name.to_ascii_lowercase();
            if !lower
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                anyhow::bail!("KINTARA_PROXY_USER_HEADER must be a valid header name (letters, digits, '-')");
            }
            Ok(Some(lower))
        }
        (_, _) => Ok(None),
    }
}

fn proxy_admin_users_from_env() -> Vec<String> {
    std::env::var("KINTARA_PROXY_ADMIN_USERS")
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Accepts the spellings people actually type in a compose file.
fn bool_var(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

/// Colon-separated extra library roots (`PATH` convention). Empty entries are
/// skipped; relative paths resolve against the process working directory like
/// `KINTARA_LIBRARY_DIR` does. Paths containing `:` are not supported.
fn extra_dirs_var(key: &str) -> Vec<PathBuf> {
    std::env::var(key)
        .map(|raw| {
            raw.split(':')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn path_var(key: &str, default: &str) -> PathBuf {
    std::env::var(key)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}
