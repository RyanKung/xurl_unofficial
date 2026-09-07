//! Unofficial xurl-compatible client for X web GraphQL.
//!
//! Core modules parse and validate values. HTTP and filesystem access live in
//! [`auth`] and [`http`].
//!
//! The HTTP `WEB_BEARER` is the public x.com web-client token, not a user
//! cookie. Session identity is only `auth_token` + `ct0`. GraphQL query IDs in
//! `catalog.json` expire and are refreshed from live JS on 404. Write operations
//! are wired and not live-regressed. Requests impersonate Chrome TLS/HTTP2.

#![deny(missing_docs)]

pub mod auth;
pub mod browser;
pub mod bundle;
pub mod catalog;
pub mod client;
pub mod error;
pub mod http;
pub mod media;
pub mod model;
pub mod parse;
pub mod search;
pub mod types;

pub use auth::{AuthOrigin, AuthSaved, AuthStatus, CookiesFile, SessionCookies};
pub use client::XClient;
pub use error::Error;
pub use model::{DirectMessage, Media, Tweet, User};
pub use search::SortOrder;
pub use types::{parse_media_ids, MediaId, PageSize, PostId, PostText, ScreenName, SearchQuery};
