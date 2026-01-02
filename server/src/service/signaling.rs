use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use axum::{
    extract::{
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
        Path, Query,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use futures::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{mpsc, RwLock};
use crate::dao::db::MetaInfo;

#[derive(Debug, Deserialize)]
pub(crate) struct SignalQuery {
    role: String,
    rid: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Sender,
    Receiver,
}

impl Role {
    fn parse(input: &str) -> Option<Self> {
        match input {
            "sender" => Some(Role::Sender),
            "receiver" => Some(Role::Receiver),
            _ => None,
        }
    }
}

struct Peer {
    id: u64,
    tx: mpsc::UnboundedSender<Message>,
}

#[derive(Default)]
struct Room {
    sender: Option<Peer>,
    receiver: Option<Peer>,
}

impl Room {
    fn is_empty(&self) -> bool {
        self.sender.is_none() && self.receiver.is_none()
    }
}

lazy_static! {
    static ref SIGNAL_ROOMS: Arc<RwLock<HashMap<String, Room>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Deserialize, Serialize)]
struct IceServer {
    urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    credential: Option<String>,
}

#[derive(Debug, Serialize)]
struct WebRtcConfig {
    #[serde(rename = "iceServers")]
    ice_servers: Vec<IceServer>,
}

pub async fn webrtc_config() -> impl IntoResponse {
    let config = default_webrtc_config();

    Json(json!({
        "iceServers": config.ice_servers,
    }))
}

fn default_webrtc_config() -> WebRtcConfig {
    WebRtcConfig {
        ice_servers: Vec::new(),
    }
}

pub(crate) async fn signal_ws(
    Path(room_id): Path<String>,
    Query(query): Query<SignalQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let role = match Role::parse(&query.role) {
        Some(role) => role,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    if role == Role::Receiver && query.rid.as_deref().unwrap_or("").is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, room_id, role, query.rid))
}

async fn handle_socket(mut socket: WebSocket, room_id: String, role: Role, rid: Option<String>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let peer_id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);

    if !register_peer(&room_id, role, peer_id, tx.clone()).await {
        let _ = socket
            .send(Message::Text(
                json!({ "type": "error", "message": "room_taken" })
                    .to_string()
                    .into(),
            ))
            .await;
        let _ = socket.close().await;
        return;
    }

    if role == Role::Receiver {
        if let Some(rid) = rid.clone() {
            mark_receiver_state(&room_id, true, Some(&rid)).await;
        }
    }

    let (mut ws_sender, mut ws_receiver) = socket.split();

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let room_id_clone = room_id.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Text(text) => {
                    forward_message(&room_id_clone, role, text).await;
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    unregister_peer(&room_id, role, peer_id).await;

    if role == Role::Receiver {
        mark_receiver_state(&room_id, false, None).await;
    }
}

async fn register_peer(
    room_id: &str,
    role: Role,
    peer_id: u64,
    tx: mpsc::UnboundedSender<Message>,
) -> bool {
    let mut rooms = SIGNAL_ROOMS.write().await;
    let room = rooms.entry(room_id.to_string()).or_default();

    match role {
        Role::Sender => {
            if room.sender.is_some() {
                return false;
            }
            room.sender = Some(Peer { id: peer_id, tx });
        }
        Role::Receiver => {
            if room.receiver.is_some() {
                return false;
            }
            room.receiver = Some(Peer { id: peer_id, tx });
        }
    }

    true
}

async fn unregister_peer(room_id: &str, role: Role, peer_id: u64) {
    let mut rooms = SIGNAL_ROOMS.write().await;
    if let Some(room) = rooms.get_mut(room_id) {
        match role {
            Role::Sender => {
                if room.sender.as_ref().map(|peer| peer.id) == Some(peer_id) {
                    room.sender = None;
                }
            }
            Role::Receiver => {
                if room.receiver.as_ref().map(|peer| peer.id) == Some(peer_id) {
                    room.receiver = None;
                }
            }
        }

        if room.is_empty() {
            rooms.remove(room_id);
        }
    }
}

async fn forward_message(room_id: &str, role: Role, text: Utf8Bytes) {
    let target = {
        let rooms = SIGNAL_ROOMS.read().await;
        rooms.get(room_id).and_then(|room| match role {
            Role::Sender => room.receiver.as_ref().map(|peer| peer.tx.clone()),
            Role::Receiver => room.sender.as_ref().map(|peer| peer.tx.clone()),
        })
    };

    if let Some(tx) = target {
        let _ = tx.send(Message::Text(text));
    }
}

async fn mark_receiver_state(room_id: &str, is_using: bool, rid: Option<&str>) {
    let meta_info = MetaInfo::get_db().get(room_id).await;
    let Some(mut meta_info) = meta_info else {
        return;
    };

    if !is_using && meta_info.value.done {
        return;
    }

    meta_info.value.is_using = is_using;
    if let Some(rid) = rid {
        meta_info.value.used_by = rid.to_string();
    } else if !is_using {
        meta_info.value.used_by = "".to_string();
    }

    let _ = MetaInfo::get_db()
        .update(room_id, meta_info.value, meta_info.exp)
        .await;
}
