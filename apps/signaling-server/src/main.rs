//! Entangle Signaling Server
//!
//! WebSocket-based peer discovery and connection brokering.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};


use shared_protocol::{PeerId, SignalingMessage};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("signaling_server=debug".parse()?)
                .add_directive("tower_http=debug".parse()?),
        )
        .init();

    info!("Starting Entangle Signaling Server");

    let state = AppState::new();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Application state
#[derive(Clone)]
struct AppState {
    /// Connected peers: PeerId -> channel to send messages
    peers: Arc<DashMap<PeerId, PeerConnection>>,
    /// Pending connection requests: target_peer_id -> source_peer_id
    pending_connections: Arc<DashMap<PeerId, Vec<PeerId>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            pending_connections: Arc::new(DashMap::new()),
        }
    }
}

/// Peer connection handle
struct PeerConnection {
    peer_id: PeerId,
    tx: mpsc::Sender<SignalingMessage>,
    remote_addr: Option<SocketAddr>,
}

/// Health check endpoint
async fn health_handler() -> &'static str {
    "OK"
}

/// Stats endpoint
async fn stats_handler(State(state): State<AppState>) -> String {
    let peer_count = state.peers.len();
    let pending_count: usize = state.pending_connections.iter().map(|v| v.len()).sum();
    
    format!(
        r#"{{"peers": {}, "pending_connections": {}}}"#,
        peer_count, pending_count
    )
}

/// WebSocket upgrade handler
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
}

/// Handle a WebSocket connection
async fn handle_websocket(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (msg_tx, mut msg_rx) = mpsc::channel::<SignalingMessage>(100);
    
    let mut peer_id: Option<PeerId> = None;

    // Spawn task to forward messages from channel to WebSocket
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    continue;
                }
            };
            
            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Process incoming messages
    while let Some(result) = ws_rx.next().await {
        let msg = match result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<SignalingMessage>(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Invalid message format: {}", e);
                        continue;
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                match bincode::deserialize::<SignalingMessage>(&data) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Invalid binary message: {}", e);
                        continue;
                    }
                }
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Ok(Message::Close(_)) => break,
            Err(e) => {
                warn!("WebSocket error: {}", e);
                break;
            }
        };

        match msg {
            SignalingMessage::Register { peer_id: id } => {
                info!("Peer registered: {}", id);
                peer_id = Some(id);
                
                state.peers.insert(
                    id,
                    PeerConnection {
                        peer_id: id,
                        tx: msg_tx.clone(),
                        remote_addr: None,
                    },
                );
                
                // Send confirmation
                let _ = msg_tx.send(SignalingMessage::Registered { peer_id: id }).await;
                
                // Check for pending connections to this peer
                if let Some((_, pending)) = state.pending_connections.remove(&id) {
                    for from_peer in pending {
                        let _ = msg_tx.send(SignalingMessage::IncomingConnection {
                            from_peer_id: from_peer,
                        }).await;
                    }
                }
            }
            
            SignalingMessage::Connect { target_peer_id } => {
                let Some(from_id) = peer_id else {
                    warn!("Connect before register");
                    continue;
                };
                
                info!("Connection request: {} -> {}", from_id, target_peer_id);
                
                if let Some(target_peer) = state.peers.get(&target_peer_id) {
                    // Target is online, forward request
                    let _ = target_peer.tx.send(SignalingMessage::IncomingConnection {
                        from_peer_id: from_id,
                    }).await;
                } else {
                    // Target is offline, queue request
                    state.pending_connections
                        .entry(target_peer_id)
                        .or_insert_with(Vec::new)
                        .push(from_id);
                    
                    // Let the requester know
                    let _ = msg_tx.send(SignalingMessage::Error {
                        message: "Peer is offline, request queued".to_string(),
                    }).await;
                }
            }
            
            SignalingMessage::Accept { from_peer_id } => {
                let Some(acceptor_id) = peer_id else {
                    continue;
                };
                
                info!("Connection accepted: {} accepted {}", acceptor_id, from_peer_id);
                
                if let Some(requester) = state.peers.get(&from_peer_id) {
                    let _ = requester.tx.send(SignalingMessage::Connected {
                        peer_id: acceptor_id,
                    }).await;
                }
                
                let _ = msg_tx.send(SignalingMessage::Connected {
                    peer_id: from_peer_id,
                }).await;
            }
            
            SignalingMessage::Reject { from_peer_id, reason } => {
                let Some(rejector_id) = peer_id else {
                    continue;
                };
                
                info!("Connection rejected: {} rejected {} ({})", rejector_id, from_peer_id, reason);
                
                if let Some(requester) = state.peers.get(&from_peer_id) {
                    let _ = requester.tx.send(SignalingMessage::Error {
                        message: format!("Connection rejected: {}", reason),
                    }).await;
                }
            }
            
            SignalingMessage::IceCandidate { target_peer_id, candidate } => {
                let Some(from_id) = peer_id else {
                    continue;
                };
                
                debug!("ICE candidate: {} -> {}", from_id, target_peer_id);
                
                if let Some(target) = state.peers.get(&target_peer_id) {
                    let _ = target.tx.send(SignalingMessage::IceCandidate {
                        target_peer_id: from_id,
                        candidate,
                    }).await;
                }
            }
            
            SignalingMessage::Ping => {
                let _ = msg_tx.send(SignalingMessage::Pong).await;
            }
            
            _ => {
                debug!("Unhandled message type");
            }
        }
    }

    // Cleanup on disconnect
    if let Some(id) = peer_id {
        info!("Peer disconnected: {}", id);
        state.peers.remove(&id);
        
        // Notify peers that were connected to this peer
        // (In a full implementation, we'd track active sessions)
    }

    forward_task.abort();
}
