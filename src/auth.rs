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
}

/// Cookie pair required by X web GraphQL.
///
/// Duplication is not `Clone`: a session is a capability, not a value.
pub struct SessionCookies {
    auth_token: String,
    ct0: String,
}

impl SessionCookies {
    /// Construct from already-loaded cookie values.
    pub fn new(auth_token: String, ct0: String) -> Result<Self, Error> {
        if auth_token.trim().is_empty() {
            return Err(Error::MissingAuth(AuthField::AuthToken));
        }
        if ct0.trim().is_empty() {
            return Err(Error::MissingAuth(AuthField::Ct0));
        }
        Ok(Self {
            auth_token: auth_token.trim().to_string(),
            ct0: ct0.trim().to_string(),
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
        match CookiesFile::default_path() {
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
                source: AuthSource::Env,
                path: None,
            };
        }
        let file = match explicit {
            Some(file) => Some(file.clone()),
            None => CookiesFile::default_path(),
        };
        match file {
            Some(file) => status_from_file(file),
            None => AuthStatus {
                auth_token: false,
                ct0: false,
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
                    Some(path) => {
                        write!(f, "Session cookies are configured from {}.", path.display())
                    }
                    None => write!(f, "Session cookies are configured from a cookies file."),
                },
                AuthSource::None => write!(f, "Session cookies are configured."),
            }
        } else {
            writeln!(f, "Session cookies are not fully configured.")?;
            writeln!(f, "  auth_token: {}", present_label(self.auth_token))?;
            writeln!(f, "  ct0: {}", present_label(self.ct0))?;
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
}

#[derive(Deserialize)]
struct CookiesToml {
    auth_token: Option<String>,
    ct0: Option<String>,
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
    let (auth_token, ct0) = if exists {
        file_fields_present(file.as_path()).unwrap_or_default()
    } else {
        (false, false)
    };
    AuthStatus {
        auth_token,
        ct0,
        source: AuthSource::File,
        path: Some(file.as_path().to_path_buf()),
    }
}

fn file_fields_present(path: &Path) -> Result<(bool, bool), Error> {
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

fn load_env() -> Option<Result<SessionCookies, Error>> {
    let token = first_env(&["TWITTER_AUTH_TOKEN", "TWITTER_COOKIE_AUTH_TOKEN"]);
    let ct0 = first_env(&["TWITTER_CT0", "TWITTER_COOKIE_CT0"]);
    match (token, ct0) {
        (None, None) => None,
        (Some(token), Some(ct0)) => Some(SessionCookies::new(token, ct0)),
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
    SessionCookies::new(token, ct0)
}

#[cfg(test)]
mod tests {
    use super::{AuthSource, CookiesFile, SessionCookies};
    use crate::error::{AuthField, Error};
    use std::fs;

    #[test]
    fn empty_token_is_missing_auth() {
        let err = SessionCookies::new(String::new(), "ct0".to_string());
        assert!(matches!(err, Err(Error::MissingAuth(AuthField::AuthToken))));
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
            assert_eq!(status.source, AuthSource::File);
            let rendered = status.to_string();
            assert!(rendered.contains("configured"));
            assert!(!rendered.contains("aaa-token"));
            assert!(!rendered.contains("bbb-ct0"));
        }
        let _ = fs::remove_file(&path);
    }
}
