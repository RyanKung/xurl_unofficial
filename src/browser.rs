//! Import `auth_token` and `ct0` from the local Chrome profile.

use pookie::enums::Cookie;

use crate::auth::SessionCookies;
use crate::error::{AuthField, Error};

/// Read X session cookies from Chrome. Values are not logged.
pub fn session_from_chrome() -> Result<SessionCookies, Error> {
    let cookies = pookie::chrome(Some(vec!["x.com".to_string(), "twitter.com".to_string()]))
        .map_err(|err| Error::ChromeImport(err.to_string()))?;
    let token =
        cookie_value(&cookies, "auth_token").ok_or(Error::MissingAuth(AuthField::AuthToken))?;
    let ct0 = cookie_value(&cookies, "ct0").ok_or(Error::MissingAuth(AuthField::Ct0))?;
    SessionCookies::new(token, ct0)
}

fn cookie_value(cookies: &[Cookie], name: &str) -> Option<String> {
    cookies
        .iter()
        .find(|cookie| is_x_session_cookie(cookie, name))
        .map(|cookie| cookie.value.clone())
}

fn is_x_session_cookie(cookie: &Cookie, name: &str) -> bool {
    cookie.name == name && host_is_x(&cookie.domain)
}

fn host_is_x(domain: &str) -> bool {
    let host = domain.trim_start_matches('.');
    host == "x.com"
        || host.ends_with(".x.com")
        || host == "twitter.com"
        || host.ends_with(".twitter.com")
}

#[cfg(test)]
mod tests {
    use super::host_is_x;

    #[test]
    fn host_is_x_accepts_x_and_twitter_domains() {
        assert!(host_is_x(".x.com"));
        assert!(host_is_x("x.com"));
        assert!(host_is_x(".twitter.com"));
        assert!(!host_is_x("example.com"));
    }
}
