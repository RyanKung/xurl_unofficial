//! Scrape rotating GraphQL query IDs from the current x.com JS bundles.

use std::collections::BTreeMap;

use regex::Regex;
use wreq::header::HeaderMap;
use wreq::Client;

use crate::error::Error;

const HOME: &str = "https://x.com";
const MAX_SCRIPTS: usize = 16;

/// Download current web JS and collect `operationName` → `queryId`.
pub async fn scrape_query_ids(
    client: &Client,
    headers: &HeaderMap,
) -> Result<BTreeMap<String, String>, Error> {
    let html = get_text(client, headers, HOME).await?;
    let mut ids = BTreeMap::new();
    for (fetched, url) in script_urls(&html).into_iter().enumerate() {
        if fetched >= MAX_SCRIPTS {
            break;
        }
        let js = get_text(client, headers, &url).await?;
        merge_query_ids(&js, &mut ids);
    }
    if ids.is_empty() {
        return Err(Error::BundleRefresh);
    }
    Ok(ids)
}

/// Parse `queryId` / `operationName` pairs from minified JS.
pub fn merge_query_ids(js: &str, ids: &mut BTreeMap<String, String>) {
    let id_then_name = Regex::new(
        r#"[\"']?queryId[\"']?\s*:\s*[\"']([A-Za-z0-9_-]+)[\"']\s*,\s*[\"']?operationName[\"']?\s*:\s*[\"']([A-Za-z0-9_]+)[\"']"#,
    );
    let name_then_id = Regex::new(
        r#"[\"']?operationName[\"']?\s*:\s*[\"']([A-Za-z0-9_]+)[\"']\s*,\s*[\"']?queryId[\"']?\s*:\s*[\"']([A-Za-z0-9_-]+)[\"']"#,
    );
    let Ok(id_then_name) = id_then_name else {
        return;
    };
    let Ok(name_then_id) = name_then_id else {
        return;
    };
    for cap in id_then_name.captures_iter(js) {
        if let (Some(query_id), Some(name)) = (cap.get(1), cap.get(2)) {
            ids.insert(name.as_str().to_string(), query_id.as_str().to_string());
        }
    }
    for cap in name_then_id.captures_iter(js) {
        if let (Some(name), Some(query_id)) = (cap.get(1), cap.get(2)) {
            ids.insert(name.as_str().to_string(), query_id.as_str().to_string());
        }
    }
}

fn script_urls(html: &str) -> Vec<String> {
    let Ok(re) = Regex::new(
        r#"(?:src|href)=["']([^"']*abs\.twimg\.com/responsive-web/client-web[^"']+\.js)["']"#,
    ) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for cap in re.captures_iter(html) {
        if let Some(raw) = cap.get(1) {
            let url = normalize_script_url(raw.as_str());
            if !urls.iter().any(|existing| existing == &url) {
                urls.push(url);
            }
        }
    }
    urls
}

fn normalize_script_url(raw: &str) -> String {
    if raw.starts_with("https://") {
        raw.to_string()
    } else if let Some(rest) = raw.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        raw.to_string()
    }
}

async fn get_text(client: &Client, headers: &HeaderMap, url: &str) -> Result<String, Error> {
    let response = client.get(url).headers(headers.clone()).send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(Error::GraphQlStatus {
            status: status.as_u16(),
            body: crate::error::body_preview(&body, 800),
        });
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::{merge_query_ids, script_urls};
    use std::collections::BTreeMap;

    #[test]
    fn merge_query_ids_reads_both_key_orders() {
        let js = r#"
            {queryId:"Gb-d6r0vxPOADdG62OEBpQ",operationName:"UserByScreenName"}
            {operationName:"SearchTimeline",queryId:"AAdH8Xl-_uS9MNpSHaSQ7A"}
        "#;
        let mut ids = BTreeMap::new();
        merge_query_ids(js, &mut ids);
        assert_eq!(
            ids.get("UserByScreenName").map(String::as_str),
            Some("Gb-d6r0vxPOADdG62OEBpQ")
        );
        assert_eq!(
            ids.get("SearchTimeline").map(String::as_str),
            Some("AAdH8Xl-_uS9MNpSHaSQ7A")
        );
    }

    #[test]
    fn script_urls_normalizes_protocol_relative_abs_twimg() {
        let html =
            r#"<script src="//abs.twimg.com/responsive-web/client-web/main.abc.js"></script>"#;
        let urls = script_urls(html);
        assert_eq!(
            urls.first().map(String::as_str),
            Some("https://abs.twimg.com/responsive-web/client-web/main.abc.js")
        );
    }
}
