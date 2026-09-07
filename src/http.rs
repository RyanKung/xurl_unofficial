//! HTTP adapter for X web GraphQL and REST verify_credentials.

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::Client;
use serde_json::{json, Value};

use crate::auth::SessionCookies;
use crate::catalog::{Catalog, GraphQlMethod, Operation};
use crate::error::{body_preview, Error};

const ORIGIN: &str = "https://x.com";
const API_ORIGIN: &str = "https://api.x.com";
const UPLOAD_ORIGIN: &str = "https://upload.x.com";
/// Public x.com web-client Bearer. This is not a user credential.
/// Session identity is only `auth_token` + `ct0` cookies.
const WEB_BEARER: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs=1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";

/// Cookie-authenticated HTTP client.
pub struct Http {
    client: Client,
    catalog: Catalog,
    headers: HeaderMap,
}

impl Http {
    /// Build a client from session cookies and the bundled catalog.
    pub fn new(session: SessionCookies) -> Result<Self, Error> {
        Ok(Self {
            client: Client::builder().build()?,
            catalog: Catalog::bundled()?,
            headers: session_headers(&session)?,
        })
    }

    /// Execute a named GraphQL operation.
    pub async fn graphql(&self, name: &'static str, variables: Value) -> Result<Value, Error> {
        let op = self.catalog.get(name)?;
        self.execute(op, variables).await
    }

    /// GET `https://api.x.com/1.1/account/verify_credentials.json`.
    pub async fn verify_credentials(&self) -> Result<Value, Error> {
        self.get_json(&format!("{API_ORIGIN}/1.1/account/verify_credentials.json"))
            .await
    }

    /// GET `https://api.x.com/1.1/account/settings.json`.
    pub async fn account_settings(&self) -> Result<Value, Error> {
        self.get_json(&format!("{API_ORIGIN}/1.1/account/settings.json"))
            .await
    }

    /// POST `application/x-www-form-urlencoded` to an x.com path.
    pub async fn form_post(&self, path: &'static str, body: &str) -> Result<Value, Error> {
        let url = format!("{ORIGIN}{path}");
        let mut headers = self.headers.clone();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        let response = self
            .client
            .post(url)
            .headers(headers)
            .body(body.to_string())
            .send()
            .await?;
        read_json(response).await
    }

    /// GET an x.com API path.
    pub async fn get_path(&self, path: &'static str) -> Result<Value, Error> {
        self.get_json(&format!("{ORIGIN}{path}")).await
    }

    /// POST urlencoded fields, letting reqwest encode values.
    pub async fn form_pairs(
        &self,
        path: &'static str,
        pairs: &[(&str, String)],
    ) -> Result<Value, Error> {
        let url = format!("{ORIGIN}{path}");
        let mut headers = self.headers.clone();
        headers.remove(CONTENT_TYPE);
        let response = self
            .client
            .post(url)
            .headers(headers)
            .form(pairs)
            .send()
            .await?;
        read_json(response).await
    }

    /// INIT a media upload.
    pub async fn media_init(
        &self,
        total_bytes: u64,
        media_type: &str,
        media_category: &str,
    ) -> Result<Value, Error> {
        let url = format!("{UPLOAD_ORIGIN}/i/media/upload.json");
        let mut headers = self.headers.clone();
        headers.remove(CONTENT_TYPE);
        let total = total_bytes.to_string();
        let response = self
            .client
            .post(url)
            .headers(headers)
            .form(&[
                ("command", "INIT"),
                ("total_bytes", total.as_str()),
                ("media_type", media_type),
                ("media_category", media_category),
            ])
            .send()
            .await?;
        read_json(response).await
    }

    /// APPEND one media segment.
    pub async fn media_append(
        &self,
        media_id: &str,
        segment_index: u32,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        let url = format!("{UPLOAD_ORIGIN}/i/media/upload.json");
        let mut headers = self.headers.clone();
        headers.remove(CONTENT_TYPE);
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name("blob")
            .mime_str("application/octet-stream")
            .map_err(|_| Error::InvalidMedia)?;
        let form = reqwest::multipart::Form::new()
            .text("command", "APPEND")
            .text("media_id", media_id.to_string())
            .text("segment_index", segment_index.to_string())
            .part("media", part);
        let response = self
            .client
            .post(url)
            .headers(headers)
            .multipart(form)
            .send()
            .await?;
        read_json_allow_empty(response).await?;
        Ok(())
    }

    /// FINALIZE a media upload.
    pub async fn media_finalize(&self, media_id: &str) -> Result<Value, Error> {
        let url = format!("{UPLOAD_ORIGIN}/i/media/upload.json");
        let mut headers = self.headers.clone();
        headers.remove(CONTENT_TYPE);
        let response = self
            .client
            .post(url)
            .headers(headers)
            .form(&[("command", "FINALIZE"), ("media_id", media_id)])
            .send()
            .await?;
        read_json(response).await
    }

    /// STATUS a media upload.
    pub async fn media_status(&self, media_id: &str) -> Result<Value, Error> {
        let url = format!("{UPLOAD_ORIGIN}/i/media/upload.json");
        let mut headers = self.headers.clone();
        headers.remove(CONTENT_TYPE);
        let response = self
            .client
            .get(url)
            .headers(headers)
            .query(&[("command", "STATUS"), ("media_id", media_id)])
            .send()
            .await?;
        read_json(response).await
    }

    async fn get_json(&self, url: &str) -> Result<Value, Error> {
        let response = self
            .client
            .get(url)
            .headers(self.headers.clone())
            .send()
            .await?;
        read_json(response).await
    }

    async fn execute(&self, op: &Operation, variables: Value) -> Result<Value, Error> {
        let url = format!("{ORIGIN}/i/api/graphql/{}/{}", op.query_id, op.name);
        let request = match op.method {
            GraphQlMethod::Get => {
                let params = [
                    ("variables", variables.to_string()),
                    ("features", op.features.to_string()),
                ];
                self.client.get(url).query(&params)
            }
            GraphQlMethod::Post => {
                let body = json!({
                    "queryId": op.query_id,
                    "variables": variables,
                    "features": op.features,
                });
                self.client.post(url).json(&body)
            }
        };
        let response = request.headers(self.headers.clone()).send().await?;
        read_json(response).await
    }
}

fn session_headers(session: &SessionCookies) -> Result<HeaderMap, Error> {
    let cookie = format!("auth_token={}; ct0={}", session.auth_token(), session.ct0());
    let mut headers = HeaderMap::new();
    insert_static(&mut headers, "authorization", WEB_BEARER)?;
    insert_owned(&mut headers, "cookie", &cookie)?;
    insert_owned(&mut headers, "x-csrf-token", session.ct0())?;
    insert_static(&mut headers, "origin", ORIGIN)?;
    insert_static(&mut headers, "referer", "https://x.com/")?;
    insert_static(
        &mut headers,
        "user-agent",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    )?;
    insert_static(&mut headers, "x-twitter-auth-type", "OAuth2Session")?;
    insert_static(&mut headers, "x-twitter-active-user", "yes")?;
    insert_static(&mut headers, "x-twitter-client-language", "en")?;
    insert_static(&mut headers, "content-type", "application/json")?;
    Ok(headers)
}

fn insert_static(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &'static str,
) -> Result<(), Error> {
    let header_name = HeaderName::from_static(name);
    let header_value = HeaderValue::from_static(value);
    headers.insert(header_name, header_value);
    Ok(())
}

fn insert_owned(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), Error> {
    let header_name = HeaderName::from_static(name);
    let header_value = HeaderValue::try_from(value).map_err(|_| Error::InvalidHeader)?;
    headers.insert(header_name, header_value);
    Ok(())
}

async fn read_json(response: reqwest::Response) -> Result<Value, Error> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(Error::GraphQlStatus {
            status: status.as_u16(),
            body: body_preview(&body, 800),
        });
    }
    Ok(serde_json::from_str(&body)?)
}

async fn read_json_allow_empty(response: reqwest::Response) -> Result<Value, Error> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(Error::GraphQlStatus {
            status: status.as_u16(),
            body: body_preview(&body, 800),
        });
    }
    if body.trim().is_empty() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(&body)?)
}
