//! Signaling client for Entangle
//!
//! Handles WebSocket connection to the signaling server for peer discovery
//! and connection establishment.

use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use shared_protocol::{PeerId, SignalingMessage};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};

/// Signaling client error
#[derive(Debug, thiserror::Error)]
pub enum SignalingError {
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Socket error: {0}")]
    Socket(String),
    #[error("Channel error")]
    Channel,
}

pub type SignalingResult<T> = Result<T, SignalingError>;

/// Signaling client
pub struct SignalingClient {
    peer_id: PeerId,
    server_url: String,
    running: Arc<Mutex<bool>>,
}

impl SignalingClient {
    /// Create a new signaling client
    pub fn new(peer_id: PeerId, server_url: String) -> Self {
        Self {
            peer_id,
            server_url,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Connect to the signaling server
    /// Returns a channel receiver for incoming messages from the server
    pub async fn connect(
        &self,
    ) -> SignalingResult<(
        mpsc::Sender<SignalingMessage>,
        mpsc::Receiver<SignalingMessage>,
    )> {
        info!("Connecting to signaling server: {}", self.server_url);

        let (ws_stream, _) = connect_async(&self.server_url)
            .await
            .map_err(|e| SignalingError::Connection(e.to_string()))?;

        info!("WebSocket connected");

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        // Channel for sending messages TO the server
        let (send_tx, mut send_rx) = mpsc::channel::<SignalingMessage>(100);

        // Channel for receiving messages FROM the server
        let (recv_tx, recv_rx) = mpsc::channel::<SignalingMessage>(100);

        // Register immediately
        let register_msg = SignalingMessage::Register {
            peer_id: self.peer_id,
        };
        let register_json = serde_json::to_string(&register_msg)
            .map_err(|e| SignalingError::Connection(e.to_string()))?;

        ws_tx
            .send(Message::Text(register_json.into()))
            .await
            .map_err(|e| SignalingError::Socket(e.to_string()))?;

        // Spawn background task to handle WebSocket I/O
        let running = self.running.clone();
        *running.lock().await = true;

        tokio::spawn(async move {
            loop {
                // Determine if we should stop
                if !*running.lock().await {
                    break;
                }

                tokio::select! {
                    // Outgoing: Application -> Server
                    Some(msg) = send_rx.recv() => {
                        let json = match serde_json::to_string(&msg) {
                            Ok(j) => j,
                            Err(e) => {
                                error!("Failed to serialize outgoing message: {}", e);
                                continue;
                            }
                        };

                        if let Err(e) = ws_tx.send(Message::Text(json.into())).await {
                            error!("Failed to send WebSocket message: {}", e);
                            break;
                        }
                    }

                    // Incoming: Server -> Application
                    Some(msg) = ws_rx.next() => {
                         match msg {
                            Ok(Message::Text(text)) => {
                                match serde_json::from_str::<SignalingMessage>(&text) {
                                    Ok(parsed) => {
                                        if let Err(e) = recv_tx.send(parsed).await {
                                            warn!("Failed to forward incoming message: {}", e);
                                            break;
                                        }
                                    }
                                    Err(e) => warn!("Failed to parse incoming message: {}", e),
                                }
                            }
                            Ok(Message::Binary(data)) => {
                                // Try bincode if binary
                                match bincode::deserialize::<SignalingMessage>(&data) {
                                    Ok(parsed) => {
                                        if let Err(e) = recv_tx.send(parsed).await {
                                           warn!("Failed to forward incoming binary message: {}", e);
                                           break;
                                        }
                                    }
                                    Err(e) => warn!("Failed to parse incoming binary message: {}", e),
                                }
                            }
                            Ok(Message::Close(_)) => {
                                info!("Server closed connection");
                                break;
                            }
                            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                            Ok(Message::Frame(_)) => {} // Ignore raw frames
                            Err(e) => {
                                error!("WebSocket receive error: {}", e);
                                break;
                            }
                        }
                    }

                    else => break,
                }
            }
            info!("Signaling loop ended");
        });

        Ok((send_tx, recv_rx))
    }

    pub async fn disconnect(&self) {
        *self.running.lock().await = false;
    }
}
