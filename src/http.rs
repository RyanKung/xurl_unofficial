//! HTTP adapter for X web GraphQL.
//!
//! Uses Chrome TLS/HTTP2 emulation (`wreq`). GraphQL 404 refreshes query IDs
//! from live JS. `x-client-transaction-id` is generated from the live homepage
//! when the current algorithm still parses.

use std::sync::Mutex;

use serde_json::{json, Value};
use wreq::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use wreq::Client;
use wreq_util::Emulation;
use x_client_transaction::ClientTransaction;

use crate::auth::SessionCookies;
use crate::bundle;
use crate::catalog::{Catalog, GraphQlMethod, Operation};
use crate::error::{body_preview, Error};

const ORIGIN: &str = "https://x.com";
const API_ORIGIN: &str = "https://api.x.com";
const UPLOAD_ORIGIN: &str = "https://upload.x.com";
/// Public x.com web-client Bearer. This is not a user credential.
/// Session identity is only `auth_token` + `ct0` cookies.
const WEB_BEARER: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs=1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";

enum TidState {
    Untried,
    Ready(ClientTransaction),
    Failed,
}

/// Cookie-authenticated HTTP client.
pub struct Http {
    client: Client,
    catalog: Mutex<Catalog>,
    headers: HeaderMap,
    tid: Mutex<TidState>,
}

impl Http {
    /// Build a Chrome-emulated client from session cookies and the bundled catalog.
    pub fn new(session: SessionCookies) -> Result<Self, Error> {
        Ok(Self {
            client: Client::builder().emulation(Emulation::Chrome149).build()?,
            catalog: Mutex::new(Catalog::bundled()?),
            headers: session_headers(&session)?,
            tid: Mutex::new(TidState::Untried),
        })
    }

    /// Execute a named GraphQL operation, refreshing query IDs once on 404.
    pub async fn graphql(&self, name: &'static str, variables: Value) -> Result<Value, Error> {
        let op = self.operation(name)?;
        match self.execute(&op, variables.clone()).await {
            Err(Error::GraphQlStatus { status: 404, .. }) => {
                self.refresh_query_ids().await?;
                let op = self.operation(name)?;
                self.execute(&op, variables).await
            }
            other => other,
        }
    }

    /// GET `https://api.x.com/1.1/account/verify_credentials.json`.
    pub async fn verify_credentials(&self) -> Result<Value, Error> {
        self.get_json(
            &format!("{API_ORIGIN}/1.1/account/verify_credentials.json"),
            "/1.1/account/verify_credentials.json",
        )
        .await
    }

    /// GET `https://api.x.com/1.1/account/settings.json`.
    pub async fn account_settings(&self) -> Result<Value, Error> {
        self.get_json(
            &format!("{API_ORIGIN}/1.1/account/settings.json"),
            "/1.1/account/settings.json",
        )
        .await
    }

    /// POST `application/x-www-form-urlencoded` to an x.com path.
    pub async fn form_post(&self, path: &'static str, body: &str) -> Result<Value, Error> {
        let url = format!("{ORIGIN}{path}");
        let mut headers = self.headers_for("POST", path).await?;
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
        self.get_json(&format!("{ORIGIN}{path}"), path).await
    }

    /// POST urlencoded fields, letting the client encode values.
    pub async fn form_pairs(
        &self,
        path: &'static str,
        pairs: &[(&str, String)],
    ) -> Result<Value, Error> {
        let url = format!("{ORIGIN}{path}");
        let mut headers = self.headers_for("POST", path).await?;
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
        let path = "/i/media/upload.json";
        let url = format!("{UPLOAD_ORIGIN}{path}");
        let mut headers = self.headers_for("POST", path).await?;
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
        let path = "/i/media/upload.json";
        let url = format!("{UPLOAD_ORIGIN}{path}");
        let mut headers = self.headers_for("POST", path).await?;
        headers.remove(CONTENT_TYPE);
        let part = wreq::multipart::Part::bytes(bytes)
            .file_name("blob")
            .mime_str("application/octet-stream")
            .map_err(|_| Error::InvalidMedia)?;
        let form = wreq::multipart::Form::new()
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
        let path = "/i/media/upload.json";
        let url = format!("{UPLOAD_ORIGIN}{path}");
        let mut headers = self.headers_for("POST", path).await?;
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
        let path = "/i/media/upload.json";
        let url = format!("{UPLOAD_ORIGIN}{path}");
        let mut headers = self.headers_for("GET", path).await?;
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

    async fn get_json(&self, url: &str, path: &str) -> Result<Value, Error> {
        let headers = self.headers_for("GET", path).await?;
        let response = self.client.get(url).headers(headers).send().await?;
        read_json(response).await
    }

    async fn execute(&self, op: &Operation, variables: Value) -> Result<Value, Error> {
        let path = format!("/i/api/graphql/{}/{}", op.query_id, op.name);
        let url = format!("{ORIGIN}{path}");
        let method = match op.method {
            GraphQlMethod::Get => "GET",
            GraphQlMethod::Post => "POST",
        };
        let headers = self.headers_for(method, &path).await?;
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
        let response = request.headers(headers).send().await?;
        read_json(response).await
    }

    fn operation(&self, name: &'static str) -> Result<Operation, Error> {
        let catalog = self
            .catalog
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        catalog.get(name).cloned()
    }

    async fn refresh_query_ids(&self) -> Result<(), Error> {
        let ids = bundle::scrape_query_ids(&self.client, &self.headers).await?;
        let mut catalog = self
            .catalog
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let changed = catalog.apply_query_ids(&ids);
        if changed == 0 {
            return Err(Error::BundleRefresh);
        }
        Ok(())
    }

    async fn headers_for(&self, method: &str, path: &str) -> Result<HeaderMap, Error> {
        let mut headers = self.headers.clone();
        if let Some(tid) = self.transaction_id(method, path).await? {
            insert_owned(&mut headers, "x-client-transaction-id", &tid)?;
        }
        Ok(headers)
    }

    async fn transaction_id(&self, method: &str, path: &str) -> Result<Option<String>, Error> {
        {
            let state = self
                .tid
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &*state {
                TidState::Failed => return Ok(None),
                TidState::Ready(tx) => {
                    return tx
                        .generate_transaction_id(method, path)
                        .map(Some)
                        .map_err(|err| Error::Transaction(err.to_string()));
                }
                TidState::Untried => {}
            }
        }
        let init = tokio::task::spawn_blocking(init_tid).await;
        let mut state = self
            .tid
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match init {
            Ok(Ok(tx)) => {
                let header = tx
                    .generate_transaction_id(method, path)
                    .map_err(|err| Error::Transaction(err.to_string()))?;
                *state = TidState::Ready(tx);
                Ok(Some(header))
            }
            Ok(Err(_)) | Err(_) => {
                *state = TidState::Failed;
                Ok(None)
            }
        }
    }
}

fn init_tid() -> Result<ClientTransaction, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36",
        )
        .build()
        .map_err(|err| Error::Transaction(err.to_string()))?;
    ClientTransaction::new(&client).map_err(|err| Error::Transaction(err.to_string()))
}

fn session_headers(session: &SessionCookies) -> Result<HeaderMap, Error> {
    let cookie = format!("auth_token={}; ct0={}", session.auth_token(), session.ct0());
    let mut headers = HeaderMap::new();
    insert_static(&mut headers, "authorization", WEB_BEARER)?;
    insert_owned(&mut headers, "cookie", &cookie)?;
    insert_owned(&mut headers, "x-csrf-token", session.ct0())?;
    insert_static(&mut headers, "origin", ORIGIN)?;
    insert_static(&mut headers, "referer", "https://x.com/")?;
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

async fn read_json(response: wreq::Response) -> Result<Value, Error> {
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

async fn read_json_allow_empty(response: wreq::Response) -> Result<Value, Error> {
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
