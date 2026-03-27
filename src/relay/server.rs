use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot};

use crate::protocol::messages::{
    PAGE_CHUNK_TYPE, PageChunk, RawSnapshot, Request, Response, actions,
};

use super::native_msg;

const RELAY_ADDR: &str = "127.0.0.1:12899";

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<Response>>>>;
type SessionMap = Arc<Mutex<HashMap<String, SessionCache>>>;

#[derive(Debug, Clone)]
struct SessionCache {
    snapshot: Option<RawSnapshot>,
    complete: bool,
    /// request_id of the snapshot currently being assembled.
    /// Chunks whose request_id doesn't match are stale (from a superseded operation) and discarded.
    active_request_id: Option<String>,
}

pub async fn run() -> Result<()> {
    let listener = TcpListener::bind(RELAY_ADDR).await?;
    eprintln!("relay: listening on {RELAY_ADDR}");

    let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
    let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

    let pending_clone = Arc::clone(&pending);
    let sessions_clone = Arc::clone(&sessions);
    tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        loop {
            match native_msg::read_message(&mut stdin).await {
                Ok(msg) => {
                    if msg.get("type").and_then(|value| value.as_str()) == Some(PAGE_CHUNK_TYPE) {
                        if let Err(err) = handle_page_chunk(msg, &sessions_clone).await {
                            eprintln!("relay: failed to handle page chunk: {err}");
                        }
                        continue;
                    }

                    let response: Response = match serde_json::from_value(msg.clone()) {
                        Ok(response) => response,
                        Err(err) => {
                            eprintln!("relay: invalid native response: {err}");
                            continue;
                        }
                    };

                    let mut map = pending_clone.lock().await;
                    if let Some(tx) = map.remove(&response.id) {
                        let _ = tx.send(response);
                    } else {
                        eprintln!("relay: no pending request for id={}", response.id);
                    }
                }
                Err(err) => {
                    eprintln!("relay: stdin read error: {err}");
                    break;
                }
            }
        }
    });

    loop {
        let (stream, addr) = listener.accept().await?;
        let pending = Arc::clone(&pending);
        let sessions = Arc::clone(&sessions);
        let stdout = Arc::clone(&stdout);

        tokio::spawn(async move {
            if let Err(err) = handle_client(stream, pending, sessions, stdout).await {
                eprintln!("relay: client {addr} error: {err}");
            }
        });
    }
}

async fn handle_client(
    stream: tokio::net::TcpStream,
    pending: PendingMap,
    sessions: SessionMap,
    stdout: Arc<Mutex<tokio::io::Stdout>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let request: Request = serde_json::from_str(&line)?;

        if let Some(response) = cached_response(&request, &sessions).await? {
            write_response(&mut writer, &response).await?;
            continue;
        }

        let forwarded = forwarded_request(&request);
        let payload = serde_json::to_value(&forwarded)?;
        let (tx, rx) = oneshot::channel();

        pending.lock().await.insert(request.id.clone(), tx);

        {
            let mut out = stdout.lock().await;
            native_msg::write_message(&mut *out, &payload).await?;
        }

        let extension_response = match rx.await {
            Ok(resp) => resp,
            Err(_) => {
                let err = Response::error(request.id, "native response channel closed");
                write_response(&mut writer, &err).await?;
                continue;
            }
        };

        let response = finalize_response(request, extension_response, &sessions).await?;
        write_response(&mut writer, &response).await?;
    }

    Ok(())
}

async fn cached_response(request: &Request, sessions: &SessionMap) -> Result<Option<Response>> {
    match request.action.as_str() {
        // GET_PAGE_FRESH explicitly skips the cache — fall through to browser.
        actions::GET_PAGE | actions::SEARCH | actions::GET_TEXT => {
            let Some(session_id) = request
                .params
                .get("session_id")
                .and_then(|value| value.as_str())
            else {
                return Ok(None);
            };

            let sessions = sessions.lock().await;
            let Some(cache) = sessions.get(session_id) else {
                return Ok(None);
            };
            if !cache.complete {
                return Ok(None);
            }
            let Some(snapshot) = &cache.snapshot else {
                return Ok(None);
            };

            Ok(Some(Response::success(
                request.id.clone(),
                serde_json::to_value(snapshot)?,
            )))
        }
        _ => Ok(None),
    }
}

fn forwarded_request(request: &Request) -> Request {
    if matches!(
        request.action.as_str(),
        actions::SEARCH | actions::GET_TEXT | actions::GET_PAGE_FRESH
    ) {
        Request {
            id: request.id.clone(),
            action: actions::GET_PAGE.into(),
            params: request.params.clone(),
        }
    } else {
        request.clone()
    }
}

async fn finalize_response(
    request: Request,
    extension_response: Response,
    sessions: &SessionMap,
) -> Result<Response> {
    if !extension_response.is_success() {
        return Ok(extension_response);
    }

    match request.action.as_str() {
        actions::GET_PAGE | actions::GET_PAGE_FRESH | actions::SEARCH | actions::GET_TEXT => {
            if let Some(session_id) = request
                .params
                .get("session_id")
                .and_then(|value| value.as_str())
            {
                if let Some(snapshot) = snapshot_for_session(sessions, session_id).await {
                    return Ok(Response::success(
                        request.id,
                        serde_json::to_value(snapshot)?,
                    ));
                }
            }
            Ok(extension_response)
        }
        actions::CLOSE => {
            if request.params.get("all").and_then(|value| value.as_bool()) == Some(true) {
                sessions.lock().await.clear();
            } else if let Some(session_id) = request
                .params
                .get("session_id")
                .and_then(|value| value.as_str())
            {
                sessions.lock().await.remove(session_id);
            }
            Ok(extension_response)
        }
        _ => Ok(extension_response),
    }
}

async fn snapshot_for_session(sessions: &SessionMap, session_id: &str) -> Option<RawSnapshot> {
    let sessions = sessions.lock().await;
    let cache = sessions.get(session_id)?;
    if !cache.complete {
        return None;
    }
    cache.snapshot.clone()
}

async fn handle_page_chunk(msg: serde_json::Value, sessions: &SessionMap) -> Result<()> {
    let chunk: PageChunk = serde_json::from_value(msg)?;
    let mut sessions = sessions.lock().await;
    let cache = sessions
        .entry(chunk.session_id.clone())
        .or_insert(SessionCache {
            snapshot: None,
            complete: false,
            active_request_id: None,
        });

    if chunk.chunk_index == 0 || chunk.meta.is_some() || cache.snapshot.is_none() {
        let Some(meta) = chunk.meta.clone() else {
            anyhow::bail!("first page chunk for session {} is missing meta", chunk.session_id);
        };
        cache.snapshot = Some(RawSnapshot::from_meta(meta));
        cache.complete = false;
        cache.active_request_id = Some(chunk.request_id.clone());
    } else if cache.active_request_id.as_deref() != Some(chunk.request_id.as_str()) {
        // Stale chunk from a superseded operation — discard to prevent cross-operation data mixing.
        eprintln!(
            "relay: discarding stale chunk (session={}, expected={:?}, got={})",
            chunk.session_id,
            cache.active_request_id,
            chunk.request_id,
        );
        return Ok(());
    }

    if let Some(snapshot) = &mut cache.snapshot {
        snapshot.nodes.extend(chunk.nodes);
        cache.complete = chunk.done;
    }

    Ok(())
}

async fn write_response(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    response: &Response,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(response)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::{Rect, ScrollState, SnapshotMeta, Viewport};

    #[tokio::test]
    async fn cached_get_page_returns_snapshot() {
        let sessions: SessionMap = Arc::new(Mutex::new(HashMap::from([(
            "s1".into(),
            SessionCache {
                snapshot: Some(RawSnapshot {
                    url: "https://example.com".into(),
                    title: "Example".into(),
                    viewport: Viewport {
                        width: 1280.0,
                        height: 720.0,
                    },
                    scroll: ScrollState {
                        top: 0.0,
                        height: 1440.0,
                    },
                    nodes: vec![],
                }),
                complete: true,
                active_request_id: Some("req-1".into()),
            },
        )])));

        let response = cached_response(
            &Request {
                id: "req-1".into(),
                action: actions::GET_PAGE.into(),
                params: serde_json::json!({ "session_id": "s1" }),
            },
            &sessions,
        )
        .await
        .unwrap()
        .unwrap();

        assert!(response.ok);
        assert_eq!(response.id, "req-1");
    }

    #[tokio::test]
    async fn chunk_handler_builds_snapshot() {
        let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));
        handle_page_chunk(
            serde_json::to_value(PageChunk {
                message_type: PAGE_CHUNK_TYPE.into(),
                session_id: "s1".into(),
                request_id: "req-1".into(),
                meta: Some(SnapshotMeta {
                    url: "https://example.com".into(),
                    title: "Example".into(),
                    viewport: Viewport {
                        width: 1000.0,
                        height: 800.0,
                    },
                    scroll: ScrollState {
                        top: 0.0,
                        height: 2000.0,
                    },
                }),
                nodes: vec![crate::protocol::messages::RawNode {
                    ref_id: "r1".into(),
                    parent: None,
                    tag: "a".into(),
                    text: "Link".into(),
                    attrs: HashMap::new(),
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 10.0,
                        h: 10.0,
                    },
                }],
                chunk_index: 0,
                done: true,
            })
            .unwrap(),
            &sessions,
        )
        .await
        .unwrap();

        let snapshot = snapshot_for_session(&sessions, "s1").await.unwrap();
        assert_eq!(snapshot.nodes.len(), 1);
    }

    #[tokio::test]
    async fn stale_chunk_is_discarded() {
        let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));

        let make_chunk = |req_id: &str, chunk_index: usize, done: bool, include_meta: bool| {
            serde_json::to_value(PageChunk {
                message_type: PAGE_CHUNK_TYPE.into(),
                session_id: "s1".into(),
                request_id: req_id.to_string(),
                meta: if include_meta {
                    Some(SnapshotMeta {
                        url: "https://example.com".into(),
                        title: "Example".into(),
                        viewport: Viewport { width: 1000.0, height: 800.0 },
                        scroll: ScrollState { top: 0.0, height: 1600.0 },
                    })
                } else {
                    None
                },
                nodes: vec![crate::protocol::messages::RawNode {
                    ref_id: format!("{req_id}-node"),
                    parent: None,
                    tag: "p".into(),
                    text: req_id.to_string(),
                    attrs: HashMap::new(),
                    rect: Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 },
                }],
                chunk_index,
                done,
            })
            .unwrap()
        };

        // op1 chunk_0 starts a snapshot
        handle_page_chunk(make_chunk("op1", 0, false, true), &sessions).await.unwrap();
        // op2 chunk_0 supersedes op1 — resets the cache
        handle_page_chunk(make_chunk("op2", 0, false, true), &sessions).await.unwrap();
        // op1 chunk_1 arrives late — should be discarded
        handle_page_chunk(make_chunk("op1", 1, true, false), &sessions).await.unwrap();
        // op2 chunk_1 arrives — should be appended
        handle_page_chunk(make_chunk("op2", 1, true, false), &sessions).await.unwrap();

        let snapshot = snapshot_for_session(&sessions, "s1").await.unwrap();
        // Only op2's two nodes should be present; op1's stale chunk must be absent.
        assert_eq!(snapshot.nodes.len(), 2);
        assert!(snapshot.nodes.iter().all(|n| n.ref_id.starts_with("op2")));
    }
}
