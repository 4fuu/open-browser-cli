use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub action: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScrollState {
    pub top: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawNode {
    #[serde(rename = "ref")]
    pub ref_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub tag: String,
    pub text: String,
    #[serde(default)]
    pub attrs: HashMap<String, String>,
    pub rect: Rect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotMeta {
    pub url: String,
    pub title: String,
    pub viewport: Viewport,
    pub scroll: ScrollState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawSnapshot {
    pub url: String,
    pub title: String,
    pub viewport: Viewport,
    pub scroll: ScrollState,
    #[serde(default)]
    pub nodes: Vec<RawNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageChunk {
    #[serde(rename = "type")]
    pub message_type: String,
    pub session_id: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<SnapshotMeta>,
    #[serde(default)]
    pub nodes: Vec<RawNode>,
    pub chunk_index: usize,
    pub done: bool,
}

impl Request {
    pub fn new(action: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            action: action.into(),
            params,
        }
    }
}

impl Response {
    pub fn success(id: String, data: serde_json::Value) -> Self {
        Self {
            id,
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(id: String, msg: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }

    pub fn is_success(&self) -> bool {
        self.ok
    }

    pub fn into_result(self) -> anyhow::Result<serde_json::Value> {
        if self.ok {
            Ok(self.data.unwrap_or(serde_json::Value::Null))
        } else {
            Err(anyhow::anyhow!(
                self.error.unwrap_or_else(|| "unknown error".into())
            ))
        }
    }
}

impl RawSnapshot {
    pub fn from_meta(meta: SnapshotMeta) -> Self {
        Self {
            url: meta.url,
            title: meta.title,
            viewport: meta.viewport,
            scroll: meta.scroll,
            nodes: Vec::new(),
        }
    }
}

pub mod actions {
    pub const OPEN: &str = "open";
    pub const CLOSE: &str = "close";
    pub const LIST: &str = "list";
    pub const GET_PAGE: &str = "get_page";
    /// Same as GET_PAGE but bypasses the Relay cache and always fetches a fresh snapshot.
    pub const GET_PAGE_FRESH: &str = "get_page_fresh";
    pub const SEARCH: &str = "search";
    pub const CLICK: &str = "click";
    pub const TYPE: &str = "type";
    pub const WAIT: &str = "wait";
    pub const GET_TEXT: &str = "get_text";
    pub const SCREENSHOT: &str = "screenshot";
    pub const DOWNLOAD: &str = "download";
}

pub const PAGE_CHUNK_TYPE: &str = "page_chunk";
pub const DOWNLOAD_CHUNK_TYPE: &str = "download_chunk";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DownloadChunk {
    #[serde(rename = "type")]
    pub message_type: String,
    pub session_id: String,
    pub request_id: String,
    pub chunk_index: usize,
    pub data: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_new_generates_unique_ids() {
        let r1 = Request::new("open", json!({}));
        let r2 = Request::new("open", json!({}));
        assert_ne!(r1.id, r2.id);
    }

    #[test]
    fn response_into_result_error() {
        let resp = Response::error("id1".into(), "bad request");
        let result = resp.into_result();
        assert_eq!(result.unwrap_err().to_string(), "bad request");
    }

    #[test]
    fn raw_snapshot_from_meta_starts_empty() {
        let snapshot = RawSnapshot::from_meta(SnapshotMeta {
            url: "https://example.com".into(),
            title: "Example".into(),
            viewport: Viewport {
                width: 1200.0,
                height: 800.0,
            },
            scroll: ScrollState {
                top: 0.0,
                height: 2000.0,
            },
        });

        assert_eq!(snapshot.url, "https://example.com");
        assert!(snapshot.nodes.is_empty());
    }

    #[test]
    fn page_chunk_round_trip() {
        let chunk = PageChunk {
            message_type: PAGE_CHUNK_TYPE.into(),
            session_id: "s1".into(),
            request_id: "req-1".into(),
            meta: None,
            nodes: vec![RawNode {
                ref_id: "r1".into(),
                parent: None,
                tag: "a".into(),
                text: "Sign In".into(),
                attrs: HashMap::from([(String::from("href"), String::from("/login"))]),
                rect: Rect {
                    x: 10.0,
                    y: 20.0,
                    w: 30.0,
                    h: 40.0,
                },
            }],
            chunk_index: 0,
            done: true,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        let parsed: PageChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, chunk);
    }
}
