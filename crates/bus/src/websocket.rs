//! WebSocket transport for bidirectional agent communication.
//!
//! Provides a WebSocket server alongside the existing gRPC/SSE endpoints.
//! Clients can connect, subscribe to topics, publish messages, and receive
//! real-time agent events over a single persistent connection.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tracing::{error, info, warn};

use crate::{AgentBusTrait, AgentEnvelope};

/// WebSocket message types for the agent bus protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsMessage {
    /// Subscribe to one or more topics.
    Subscribe { topics: Vec<String> },
    /// Unsubscribe from topics.
    Unsubscribe { topics: Vec<String> },
    /// Publish a message to a topic.
    Publish { topic: String, payload: Vec<u8> },
    /// An event received from the bus.
    Event {
        topic: String,
        source: String,
        payload: Vec<u8>,
        timestamp: u64,
    },
    /// Error message.
    Error { message: String },
}

/// A WebSocket client connection.
struct WsClient {
    sender: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        tokio_tungstenite::tungstenite::Message,
    >,
    topics: Vec<String>,
    agent_id: String,
}

/// WebSocket server for real-time agent communication.
pub struct WsServer {
    bus: Arc<dyn AgentBusTrait>,
    clients: Arc<RwLock<Vec<WsClient>>>,
    event_tx: broadcast::Sender<(String, AgentEnvelope)>,
    allowed_origins: HashSet<String>,
}

impl WsServer {
    pub fn new(bus: Arc<dyn AgentBusTrait>) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            bus,
            clients: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            allowed_origins: HashSet::new(),
        }
    }

    /// Restrict WebSocket connections to specific Origin headers.
    /// Call with an empty set (default) to allow all origins (insecure).
    pub fn with_allowed_origins(
        mut self,
        origins: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_origins = origins.into_iter().map(Into::into).collect();
        self
    }

    /// Start the WebSocket server on the given address.
    pub async fn start(self: Arc<Self>, addr: SocketAddr) -> Result<(), std::io::Error> {
        let listener = TcpListener::bind(addr).await?;
        info!(address = %addr, "WebSocket server started");

        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    info!(peer = %peer, "WebSocket client connecting");
                    let server = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = server.handle_connection(stream, peer).await {
                            warn!(peer = %peer, error = %e, "WebSocket connection error");
                        }
                    });
                }
                Err(e) => {
                    error!("WebSocket accept error: {e}");
                }
            }
        }
    }

    async fn handle_connection(
        self: &Arc<Self>,
        stream: tokio::net::TcpStream,
        peer: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let allowed_origins = self.allowed_origins.clone();
        let ws_stream =
            tokio_tungstenite::accept_hdr_async(stream, OriginValidator { allowed_origins })
                .await?;
        let (ws_sender, mut ws_receiver) = ws_stream.split();

        let agent_id = format!("ws-{}", chrono::Utc::now().timestamp_millis());

        // Register client
        {
            let mut clients = self.clients.write().await;
            clients.push(WsClient {
                sender: ws_sender,
                topics: vec!["broadcast".into()],
                agent_id: agent_id.clone(),
            });
        }

        info!(agent = %agent_id, peer = %peer, "WebSocket client registered");

        // Subscribe to broadcast events
        let mut event_rx = self.event_tx.subscribe();

        loop {
            tokio::select! {
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(msg)) => {
                            if let Err(e) = self.handle_message(&agent_id, msg).await {
                                warn!(agent = %agent_id, error = %e, "message handling error");
                                let mut clients = self.clients.write().await;
                                clients.retain(|c| c.agent_id != agent_id);
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            warn!(agent = %agent_id, error = %e, "WebSocket error");
                            let mut clients = self.clients.write().await;
                            clients.retain(|c| c.agent_id != agent_id);
                            break;
                        }
                        None => break,
                    }
                }
                event = event_rx.recv() => {
                    if let Ok((topic, envelope)) = event {
                        let mut clients = self.clients.write().await;
                        for client in clients.iter_mut() {
                            if client.topics.contains(&topic)
                                || topic == "broadcast"
                            {
                                let msg = serde_json::to_string(&WsMessage::Event {
                                    topic: topic.clone(),
                                    source: envelope.source_agent_id.clone(),
                                    payload: envelope.payload.clone(),
                                    timestamp: envelope.timestamp_ms,
                                }).unwrap_or_default();
                                let _ = client
                                    .sender
                                    .send(tokio_tungstenite::tungstenite::Message::Text(msg))
                                    .await;
                            }
                        }
                    }
                }
            }
        }

        let mut clients = self.clients.write().await;
        clients.retain(|c| c.agent_id != agent_id);
        info!(agent = %agent_id, "WebSocket client disconnected");

        Ok(())
    }

    async fn handle_message(
        &self,
        agent_id: &str,
        msg: tokio_tungstenite::tungstenite::Message,
    ) -> Result<(), String> {
        let text = match msg {
            tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
            tokio_tungstenite::tungstenite::Message::Binary(_data) => {
                return Err("binary messages not supported".into());
            }
            _ => return Ok(()),
        };

        let ws_msg: WsMessage = serde_json::from_str(&text).map_err(|e| e.to_string())?;

        match ws_msg {
            WsMessage::Subscribe { topics } => {
                let mut clients = self.clients.write().await;
                if let Some(client) = clients.iter_mut().find(|c| c.agent_id == agent_id) {
                    client.topics.extend(topics);
                }
            }
            WsMessage::Unsubscribe { topics } => {
                let mut clients = self.clients.write().await;
                if let Some(client) = clients.iter_mut().find(|c| c.agent_id == agent_id) {
                    client.topics.retain(|t| !topics.contains(t));
                }
            }
            WsMessage::Publish { topic, payload } => {
                let envelope = AgentEnvelope::new(agent_id, "broadcast", &topic, payload);
                let _ = self.bus.publish(envelope).await;
            }
            _ => {}
        }

        Ok(())
    }

    /// Broadcast an event to all connected WebSocket clients.
    pub async fn broadcast(&self, topic: &str, envelope: AgentEnvelope) {
        let _ = self.event_tx.send((topic.to_string(), envelope));
    }
}

/// Validates WebSocket Origin header against an allowlist.
struct OriginValidator {
    allowed_origins: HashSet<String>,
}

impl Callback for OriginValidator {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        if self.allowed_origins.is_empty() {
            return Ok(response);
        }
        let origin = request
            .headers()
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if origin.is_empty() {
            warn!("WebSocket connection rejected: missing Origin header");
            return Err(ErrorResponse::new(Some(
                "Missing Origin header".to_string(),
            )));
        }
        if !self.allowed_origins.contains(origin) {
            warn!(origin = %origin, "WebSocket connection rejected: Origin not allowed");
            return Err(ErrorResponse::new(Some(format!(
                "Origin '{origin}' not allowed"
            ))));
        }
        Ok(response)
    }
}

impl std::fmt::Debug for WsServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsServer").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::in_memory::InMemoryBus;

    #[test]
    fn test_ws_message_serialization() {
        let msg = WsMessage::Publish {
            topic: "test".into(),
            payload: vec![1, 2, 3],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Publish"));
        assert!(json.contains("test"));
    }

    #[tokio::test]
    async fn test_ws_server_creation() {
        let bus = Arc::new(InMemoryBus::new());
        let _server = WsServer::new(bus);
    }

    #[tokio::test]
    async fn test_ws_broadcast() {
        let bus = Arc::new(InMemoryBus::new());
        let server = Arc::new(WsServer::new(bus));
        let envelope = AgentEnvelope::new("alice", "bob", "test", vec![0u8; 10]);
        server.broadcast("test", envelope).await;
    }
}
