//! Bundled GraphQL operation catalog.
//!
//! Query IDs rotate when X ships a new web bundle. A 404 on a named operation
//! means `catalog.json` is stale, not that the CLI command is missing.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::error::Error;

const BUNDLED: &str = include_str!("../catalog.json");

/// HTTP method used by a GraphQL operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GraphQlMethod {
    /// GET with `variables` and `features` query params.
    Get,
    /// POST JSON body.
    Post,
}

/// One X web GraphQL operation.
#[derive(Debug, Clone)]
pub struct Operation {
    /// GraphQL operation name, e.g. `UserByScreenName`.
    pub name: String,
    /// Rotating query id.
    pub query_id: String,
    /// GET or POST.
    pub method: GraphQlMethod,
    /// Feature flags required by the current web client.
    pub features: Value,
    /// Optional field toggles required by some operations.
    pub field_toggles: Option<Value>,
}

#[derive(Deserialize)]
struct RawOp {
    #[serde(rename = "queryId")]
    query_id: String,
    method: GraphQlMethod,
    features: Value,
    #[serde(rename = "fieldToggles")]
    field_toggles: Option<Value>,
}

/// Parsed `catalog.json`.
#[derive(Debug, Clone)]
pub struct Catalog {
    ops: BTreeMap<String, Operation>,
}

impl Catalog {
    /// Load the bundled catalog.
    pub fn bundled() -> Result<Self, Error> {
        let raw: BTreeMap<String, RawOp> = serde_json::from_str(BUNDLED)
            .map_err(|_| Error::CatalogCorrupt("catalog.json is not a map of operations"))?;
        let mut ops = BTreeMap::new();
        for (name, raw_op) in raw {
            ops.insert(
                name.clone(),
                Operation {
                    name,
                    query_id: raw_op.query_id,
                    method: raw_op.method,
                    features: raw_op.features,
                    field_toggles: raw_op.field_toggles,
                },
            );
        }
        Ok(Self { ops })
    }

    /// Look up an operation by GraphQL name.
    pub fn get(&self, name: &'static str) -> Result<&Operation, Error> {
        self.ops.get(name).ok_or(Error::UnknownOperation(name))
    }

    /// Replace query IDs for operations present in `ids`. Returns how many changed.
    pub fn apply_query_ids(&mut self, ids: &BTreeMap<String, String>) -> usize {
        let mut changed = 0;
        for (name, op) in &mut self.ops {
            if let Some(query_id) = ids.get(name) {
                if op.query_id != *query_id {
                    op.query_id = query_id.clone();
                    changed += 1;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::Catalog;

    #[test]
    fn bundled_catalog_contains_core_ops() {
        let catalog = Catalog::bundled();
        assert!(catalog.is_ok());
        if let Ok(catalog) = catalog {
            assert!(catalog.get("UserByScreenName").is_ok());
            assert!(catalog.get("Viewer").is_ok());
            assert!(catalog.get("CreateTweet").is_ok());
            assert!(catalog.get("CreateNoteTweet").is_ok());
            assert!(catalog.get("FavoriteTweet").is_ok());
            assert!(catalog.get("UnfavoriteTweet").is_ok());
            assert!(catalog.get("CreateRetweet").is_ok());
            assert!(catalog.get("DeleteRetweet").is_ok());
            assert!(catalog.get("SearchTimeline").is_ok());
            assert!(catalog.get("HomeLatestTimeline").is_ok());
            assert!(catalog.get("NotificationsTimeline").is_ok());
            assert!(catalog.get("Bookmarks").is_ok());
            assert!(catalog.get("CreateBookmark").is_ok());
            assert!(catalog.get("DeleteBookmark").is_ok());
            assert!(catalog.get("Following").is_ok());
            assert!(catalog.get("Followers").is_ok());
            assert!(catalog.get("Likes").is_ok());
            assert!(catalog.get("TweetDetail").is_ok());
            assert!(catalog.get("DeleteTweet").is_ok());
        }
    }

    #[test]
    fn note_tweet_catalog_has_field_toggles() {
        let catalog = Catalog::bundled();
        assert!(catalog.is_ok());
        if let Ok(catalog) = catalog {
            let op = catalog.get("CreateNoteTweet");
            assert!(op.is_ok());
            if let Ok(op) = op {
                assert!(op.field_toggles.is_some());
            }
        }
    }

    #[test]
    fn apply_query_ids_replaces_known_ops_only() {
        let catalog = Catalog::bundled();
        assert!(catalog.is_ok());
        let Ok(mut catalog) = catalog else {
            return;
        };
        let mut ids = BTreeMap::new();
        ids.insert(
            "UserByScreenName".to_string(),
            "NewQueryIdAAAAAAAAAAAA".to_string(),
        );
        ids.insert("NotInCatalog".to_string(), "ignored".to_string());
        assert_eq!(catalog.apply_query_ids(&ids), 1);
        let found = catalog.get("UserByScreenName");
        assert!(found.is_ok());
        if let Ok(op) = found {
            assert_eq!(op.query_id.as_str(), "NewQueryIdAAAAAAAAAAAA");
        }
    }
}
