//! Session cookies. Secrets are never displayed.

use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AuthField, Error};

/// Filesystem path of a cookies.toml file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookiesFile(PathBuf);

impl CookiesFile {
    /// Wrap an explicit path.
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    /// Default location: `~/.xurl-unofficial/cookies.toml`.
    pub fn default_path() -> Option<Self> {
        env::var_os("HOME")
            .map(|home| Self(PathBuf::from(home).join(".xurl-unofficial/cookies.toml")))
    }

    /// Legacy misspelled location used by early builds.
    pub fn legacy_default_path() -> Option<Self> {
        env::var_os("HOME")
            .map(|home| Self(PathBuf::from(home).join(".xurl-unoffical/cookies.toml")))
    }

    /// Borrow the path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Use an explicit file, or `~/.xurl-unofficial/cookies.toml`.
    pub fn or_default(explicit: Option<Self>) -> Result<Self, Error> {
        match explicit {
            Some(file) => Ok(file),
            None => Self::default_path().ok_or(Error::MissingHome),
        }
    }

    /// Whether the cookies file already exists on disk.
    pub fn exists(&self) -> bool {
        self.0.exists()
    }
}

/// `y` / `yes` (any case) overwrites. Empty or `n` leaves the file unchanged.
pub fn confirms_overwrite(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Cookie capability required by X web GraphQL.
///
/// Duplication is not `Clone`: a session is a capability, not a value.
pub struct SessionCookies {
    auth_token: String,
    ct0: String,
    cookie_header: Option<String>,
}

impl SessionCookies {
    /// Construct from already-loaded cookie values.
    pub fn new(auth_token: String, ct0: String) -> Result<Self, Error> {
        Self::with_cookie_header(auth_token, ct0, None)
    }

    /// Construct from already-loaded cookie values and an optional full Cookie header.
    pub fn with_cookie_header(
        auth_token: String,
        ct0: String,
        cookie_header: Option<String>,
    ) -> Result<Self, Error> {
        let auth_token = auth_token.trim();
        let ct0 = ct0.trim();
        if auth_token.is_empty() {
            return Err(Error::MissingAuth(AuthField::AuthToken));
        }
        if ct0.is_empty() {
            return Err(Error::MissingAuth(AuthField::Ct0));
        }
        Ok(Self {
            auth_token: auth_token.to_string(),
            ct0: ct0.to_string(),
            cookie_header: cookie_header
                .and_then(|header| normalize_cookie_header(header, auth_token, ct0)),
        })
    }

    /// Load cookies from an explicit file, env vars, then the default file.
    pub fn load(explicit: Option<&CookiesFile>) -> Result<Self, Error> {
        if let Some(file) = explicit {
            return load_file(file.as_path());
        }
        if let Some(from_env) = load_env() {
            return from_env;
        }
        match default_read_path() {
            Some(default) if default.as_path().exists() => load_file(default.as_path()),
            _ => Err(Error::MissingAuth(AuthField::AuthToken)),
        }
    }

    /// Whether cookie fields are present. Never returns values.
    ///
    /// Checks env first, then the cookies file, matching [`SessionCookies::load`].
    pub fn status(explicit: Option<&CookiesFile>) -> AuthStatus {
        let env_token = env_present(&["TWITTER_AUTH_TOKEN", "TWITTER_COOKIE_AUTH_TOKEN"]);
        let env_ct0 = env_present(&["TWITTER_CT0", "TWITTER_COOKIE_CT0"]);
        if env_token || env_ct0 {
            return AuthStatus {
                auth_token: env_token,
                ct0: env_ct0,
                cookie_header: env_present(&["TWITTER_COOKIE_HEADER", "X_COOKIE_HEADER"]),
                source: AuthSource::Env,
                path: None,
            };
        }
        let file = match explicit {
            Some(file) => Some(file.clone()),
            None => default_read_path(),
        };
        match file {
            Some(file) => status_from_file(file),
            None => AuthStatus {
                auth_token: false,
                ct0: false,
                cookie_header: false,
                source: AuthSource::None,
                path: None,
            },
        }
    }

    /// Borrow `auth_token` for request construction.
    pub fn auth_token(&self) -> &str {
        &self.auth_token
    }

    /// Borrow `ct0` for CSRF and cookie headers.
    pub fn ct0(&self) -> &str {
        &self.ct0
    }

    /// Borrow the full browser Cookie header, when available.
    pub fn cookie_header(&self) -> Option<&str> {
        self.cookie_header.as_deref()
    }

    /// Whether a full browser Cookie header is configured.
    pub fn has_full_cookie_header(&self) -> bool {
        self.cookie_header.is_some()
    }

    /// Write cookies to toml. Values are never logged.
    pub fn save(&self, file: &CookiesFile) -> Result<(), Error> {
        let path = file.as_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: Some(parent.to_path_buf()),
                source,
            })?;
        }
        let body = toml::to_string_pretty(&CookiesOut {
            auth_token: &self.auth_token,
            ct0: &self.ct0,
            cookie_header: self.cookie_header.as_deref(),
        })?;
        fs::write(path, body).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;
        set_private(path)
    }
}

impl fmt::Debug for SessionCookies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionCookies")
            .field("auth_token", &"<redacted>")
            .field("ct0", &"<redacted>")
            .field("cookie_header", &self.has_full_cookie_header())
            .finish()
    }
}

/// Where session cookies were found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSource {
    /// No env vars and no cookies file.
    None,
    /// Present in process environment.
    Env,
    /// Present in the cookies toml file.
    File,
}

/// Presence-only view of configured auth. Values are never included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatus {
    /// True if auth_token is configured.
    pub auth_token: bool,
    /// True if ct0 is configured.
    pub ct0: bool,
    /// True if a full browser Cookie header is configured.
    pub cookie_header: bool,
    /// Env, file, or neither.
    pub source: AuthSource,
    /// Cookies file path when that source applies.
    pub path: Option<PathBuf>,
}

impl AuthStatus {
    /// Both required cookies are present.
    pub fn is_ready(&self) -> bool {
        self.auth_token && self.ct0
    }
}

impl fmt::Display for AuthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_ready() {
            match self.source {
                AuthSource::Env => write!(
                    f,
                    "Session cookies are configured from environment variables."
                ),
                AuthSource::File => match &self.path {
                    Some(path) => write!(
                        f,
                        "Session cookies are configured from {}. Full cookie header: {}.",
                        path.display(),
                        present_label(self.cookie_header)
                    ),
                    None => write!(
                        f,
                        "Session cookies are configured from a cookies file. Full cookie header: {}.",
                        present_label(self.cookie_header)
                    ),
                },
                AuthSource::None => write!(f, "Session cookies are configured."),
            }
        } else {
            writeln!(f, "Session cookies are not fully configured.")?;
            writeln!(f, "  auth_token: {}", present_label(self.auth_token))?;
            writeln!(f, "  ct0: {}", present_label(self.ct0))?;
            writeln!(
                f,
                "  full cookie header: {}",
                present_label(self.cookie_header)
            )?;
            match &self.path {
                Some(path) if self.source == AuthSource::File => {
                    write!(f, "  cookies file: {}", path.display())
                }
                Some(path) => write!(f, "  cookies file: {}", path.display()),
                None => write!(f, "  cookies file: not set"),
            }
        }
    }
}

fn present_label(present: bool) -> &'static str {
    if present {
        "set"
    } else {
        "missing"
    }
}

fn default_read_path() -> Option<CookiesFile> {
    let current = CookiesFile::default_path()?;
    if current.exists() {
        return Some(current);
    }
    match CookiesFile::legacy_default_path() {
        Some(legacy) if legacy.exists() => Some(legacy),
        _ => Some(current),
    }
}

/// How cookies were obtained for a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOrigin {
    /// Typed at the interactive prompt.
    Prompt,
    /// Imported from Chrome.
    Chrome,
}

impl AuthOrigin {
    fn phrase(self) -> &'static str {
        match self {
            Self::Prompt => "entered at the prompt",
            Self::Chrome => "imported from Chrome",
        }
    }
}

/// Result of writing cookies. Values are never included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSaved {
    /// Destination path.
    pub path: PathBuf,
    /// Prompt or Chrome.
    pub origin: AuthOrigin,
}

impl AuthSaved {
    /// Build a redacted save receipt.
    pub fn new(file: &CookiesFile, origin: AuthOrigin) -> Self {
        Self {
            path: file.as_path().to_path_buf(),
            origin,
        }
    }
}

impl fmt::Display for AuthSaved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Saved session cookies to {} ({}).",
            self.path.display(),
            self.origin.phrase()
        )
    }
}

#[derive(Serialize)]
struct CookiesOut<'a> {
    auth_token: &'a str,
    ct0: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cookie_header: Option<&'a str>,
}

#[derive(Deserialize)]
struct CookiesToml {
    auth_token: Option<String>,
    ct0: Option<String>,
    cookie_header: Option<String>,
    extra_cookies: Option<String>,
}

#[cfg(unix)]
fn set_private(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| Error::Io {
        path: Some(path.to_path_buf()),
        source,
    })
}

#[cfg(not(unix))]
fn set_private(_path: &Path) -> Result<(), Error> {
    Ok(())
}

fn status_from_file(file: CookiesFile) -> AuthStatus {
    let exists = file.as_path().exists();
    let (auth_token, ct0, cookie_header) = if exists {
        file_fields_present(file.as_path()).unwrap_or_default()
    } else {
        (false, false, false)
    };
    AuthStatus {
        auth_token,
        ct0,
        cookie_header,
        source: AuthSource::File,
        path: Some(file.as_path().to_path_buf()),
    }
}

fn file_fields_present(path: &Path) -> Result<(bool, bool, bool), Error> {
    let raw = fs::read_to_string(path).map_err(|source| Error::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;
    let parsed: CookiesToml = toml::from_str(&raw).map_err(|source| Error::CookiesFile {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((
        field_present(parsed.auth_token.as_deref()),
        field_present(parsed.ct0.as_deref()),
        field_present(parsed.cookie_header.as_deref())
            || field_present(parsed.extra_cookies.as_deref()),
    ))
}

fn field_present(value: Option<&str>) -> bool {
    match value {
        Some(text) => !text.trim().is_empty(),
        None => false,
    }
}

fn env_present(keys: &[&str]) -> bool {
    keys.iter().any(|key| match env::var(key) {
        Ok(value) => !value.trim().is_empty(),
        Err(_) => false,
    })
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    })
}

fn normalize_cookie_header(raw: String, auth_token: &str, ct0: &str) -> Option<String> {
    let raw = raw.trim();
    let without_prefix = raw
        .strip_prefix("Cookie:")
        .or_else(|| raw.strip_prefix("cookie:"))
        .unwrap_or(raw);
    let mut parts: Vec<String> = without_prefix
        .split(';')
        .map(str::trim)
        .filter_map(|part| {
            let (name, value) = part.split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                return None;
            }
            match name {
                "auth_token" | "ct0" => None,
                _ => Some(format!("{name}={value}")),
            }
        })
        .collect();
    parts.push(format!("auth_token={auth_token}"));
    parts.push(format!("ct0={ct0}"));
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn load_env() -> Option<Result<SessionCookies, Error>> {
    let token = first_env(&["TWITTER_AUTH_TOKEN", "TWITTER_COOKIE_AUTH_TOKEN"]);
    let ct0 = first_env(&["TWITTER_CT0", "TWITTER_COOKIE_CT0"]);
    let cookie_header = first_env(&["TWITTER_COOKIE_HEADER", "X_COOKIE_HEADER"]);
    match (token, ct0) {
        (None, None) => None,
        (Some(token), Some(ct0)) => Some(SessionCookies::with_cookie_header(
            token,
            ct0,
            cookie_header,
        )),
        (None, Some(_)) => Some(Err(Error::MissingAuth(AuthField::AuthToken))),
        (Some(_), None) => Some(Err(Error::MissingAuth(AuthField::Ct0))),
    }
}

fn load_file(path: &Path) -> Result<SessionCookies, Error> {
    let raw = fs::read_to_string(path).map_err(|source| Error::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;
    let parsed: CookiesToml = toml::from_str(&raw).map_err(|source| Error::CookiesFile {
        path: path.to_path_buf(),
        source,
    })?;
    let token = parsed
        .auth_token
        .ok_or(Error::MissingAuth(AuthField::AuthToken))?;
    let ct0 = parsed.ct0.ok_or(Error::MissingAuth(AuthField::Ct0))?;
    let cookie_header = parsed.cookie_header.or(parsed.extra_cookies);
    SessionCookies::with_cookie_header(token, ct0, cookie_header)
}

#[cfg(test)]
mod tests {
    use super::{confirms_overwrite, AuthSource, CookiesFile, SessionCookies};
    use crate::error::{AuthField, Error};
    use std::fs;

    #[test]
    fn empty_token_is_missing_auth() {
        let err = SessionCookies::new(String::new(), "ct0".to_string());
        assert!(matches!(err, Err(Error::MissingAuth(AuthField::AuthToken))));
    }

    #[test]
    fn overwrite_only_yes_or_y() {
        assert!(confirms_overwrite("y"));
        assert!(confirms_overwrite("YES"));
        assert!(!confirms_overwrite("n"));
        assert!(!confirms_overwrite(""));
        assert!(!confirms_overwrite("maybe"));
    }

    #[test]
    fn debug_redacts_secrets() {
        let session = SessionCookies::new("s3cret-auth".to_string(), "s3cret-csrf".to_string());
        assert!(session.is_ok());
        if let Ok(session) = session {
            let rendered = format!("{session:?}");
            assert!(!rendered.contains("s3cret-auth"));
            assert!(!rendered.contains("s3cret-csrf"));
            assert!(rendered.contains("<redacted>"));
        }
    }

    #[test]
    fn save_then_load_round_trip_preserves_values() {
        let path = std::env::temp_dir().join(format!(
            "xurl-cookies-{}-roundtrip.toml",
            std::process::id()
        ));
        let session = SessionCookies::new("aaa-token".to_string(), "bbb-ct0".to_string());
        assert!(session.is_ok());
        if let Ok(session) = session {
            let file = CookiesFile::new(path.clone());
            assert!(session.save(&file).is_ok());
            let loaded = SessionCookies::load(Some(&file));
            assert!(loaded.is_ok());
            if let Ok(loaded) = loaded {
                assert_eq!(loaded.auth_token, "aaa-token");
                assert_eq!(loaded.ct0, "bbb-ct0");
                assert!(!loaded.has_full_cookie_header());
            }
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn status_reads_saved_file_and_hides_values() {
        let path =
            std::env::temp_dir().join(format!("xurl-cookies-{}-status.toml", std::process::id()));
        let session = SessionCookies::new("aaa-token".to_string(), "bbb-ct0".to_string());
        assert!(session.is_ok());
        if let Ok(session) = session {
            let file = CookiesFile::new(path.clone());
            assert!(session.save(&file).is_ok());
            let status = SessionCookies::status(Some(&file));
            assert!(status.is_ready());
            assert!(!status.cookie_header);
            assert_eq!(status.source, AuthSource::File);
            let rendered = status.to_string();
            assert!(rendered.contains("configured"));
            assert!(!rendered.contains("aaa-token"));
            assert!(!rendered.contains("bbb-ct0"));
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn full_cookie_header_round_trip_is_redacted_in_status() {
        let path = std::env::temp_dir().join(format!(
            "xurl-cookies-{}-full-header.toml",
            std::process::id()
        ));
        let header = " guest_id=v1%3A123 ; auth_token=aaa-token; ct0=bbb-ct0; twid=u%3D1 ";
        let session = SessionCookies::with_cookie_header(
            "aaa-token".to_string(),
            "bbb-ct0".to_string(),
            Some(header.to_string()),
        );
        assert!(session.is_ok());
        if let Ok(session) = session {
            assert_eq!(
                session.cookie_header(),
                Some("guest_id=v1%3A123; twid=u%3D1; auth_token=aaa-token; ct0=bbb-ct0")
            );
            let file = CookiesFile::new(path.clone());
            assert!(session.save(&file).is_ok());
            let loaded = SessionCookies::load(Some(&file));
            assert!(loaded.is_ok());
            if let Ok(loaded) = loaded {
                assert!(loaded.has_full_cookie_header());
            }
            let status = SessionCookies::status(Some(&file));
            assert!(status.cookie_header);
            let rendered = status.to_string();
            assert!(rendered.contains("Full cookie header: set"));
            assert!(!rendered.contains("guest_id"));
            assert!(!rendered.contains("aaa-token"));
            assert!(!rendered.contains("bbb-ct0"));
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn full_cookie_header_strips_prefix_and_uses_canonical_auth_pair() {
        let header = "Cookie: guest_id=v1%3A123; auth_token=stale; ct0=stale; twid=u%3D1";
        let session = SessionCookies::with_cookie_header(
            "aaa-token".to_string(),
            "bbb-ct0".to_string(),
            Some(header.to_string()),
        );
        assert!(session.is_ok());
        if let Ok(session) = session {
            assert_eq!(
                session.cookie_header(),
                Some("guest_id=v1%3A123; twid=u%3D1; auth_token=aaa-token; ct0=bbb-ct0")
            );
        }
    }

    #[test]
    fn full_cookie_header_adds_required_auth_pair_when_missing() {
        let header = "guest_id=v1%3A123; twid=u%3D1";
        let session = SessionCookies::with_cookie_header(
            "aaa-token".to_string(),
            "bbb-ct0".to_string(),
            Some(header.to_string()),
        );
        assert!(session.is_ok());
        if let Ok(session) = session {
            assert_eq!(
                session.cookie_header(),
                Some("guest_id=v1%3A123; twid=u%3D1; auth_token=aaa-token; ct0=bbb-ct0")
            );
        }
    }
}
