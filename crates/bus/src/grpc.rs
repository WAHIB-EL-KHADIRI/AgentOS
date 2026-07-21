use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use http_body_util::{BodyExt, Full, Limited, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{header, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use prost::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info, warn};

use crate::{AgentBusTrait, AgentEnvelope, BusError, BusResult};

// ---------------------------------------------------------------------------
// Prost message types matching proto/agent_bus.proto
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Message)]
pub struct ProtoAgentEnvelope {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub source_agent_id: String,
    #[prost(string, tag = "3")]
    pub target_agent_id: String,
    #[prost(string, tag = "4")]
    pub topic: String,
    #[prost(bytes, tag = "5")]
    pub payload: Vec<u8>,
    #[prost(uint64, tag = "6")]
    pub timestamp_ms: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct PublishRequest {
    #[prost(message, optional, tag = "1")]
    pub envelope: Option<ProtoAgentEnvelope>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PublishResponse {
    #[prost(string, tag = "1")]
    pub message_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct SubscribeRequest {
    #[prost(string, tag = "1")]
    pub agent_id: String,
    #[prost(string, repeated, tag = "2")]
    pub topics: Vec<String>,
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

impl From<&AgentEnvelope> for ProtoAgentEnvelope {
    fn from(e: &AgentEnvelope) -> Self {
        Self {
            id: e.id.clone(),
            source_agent_id: e.source_agent_id.clone(),
            target_agent_id: e.target_agent_id.clone(),
            topic: e.topic.clone(),
            payload: e.payload.clone(),
            timestamp_ms: e.timestamp_ms,
        }
    }
}

impl From<ProtoAgentEnvelope> for AgentEnvelope {
    fn from(p: ProtoAgentEnvelope) -> Self {
        Self {
            id: p.id,
            source_agent_id: p.source_agent_id,
            target_agent_id: p.target_agent_id,
            topic: p.topic,
            payload: p.payload,
            timestamp_ms: p.timestamp_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// GrpcBusEndpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GrpcBusEndpoint {
    pub address: String,
    pub tls_enabled: bool,
}

impl GrpcBusEndpoint {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            tls_enabled: false,
        }
    }

    pub fn with_tls(mut self, enabled: bool) -> Self {
        self.tls_enabled = enabled;
        self
    }

    pub fn describe(&self) -> String {
        let scheme = if self.tls_enabled { "https" } else { "http" };
        format!("{scheme}://{}", self.address)
    }
}

// ---------------------------------------------------------------------------
// GrpcBusClient – sends protobuf-encoded messages over TCP to the server
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct GrpcBusClient {
    endpoint: GrpcBusEndpoint,
    host: String,
    port: u16,
    messages: Mutex<Vec<AgentEnvelope>>,
    subscriptions: Mutex<HashMap<String, Vec<String>>>,
}

impl GrpcBusClient {
    pub fn new(endpoint: GrpcBusEndpoint) -> Self {
        let (host, port) = parse_address(&endpoint.address);
        Self {
            endpoint,
            host,
            port,
            messages: Mutex::new(Vec::new()),
            subscriptions: Mutex::new(HashMap::new()),
        }
    }

    pub fn endpoint(&self) -> &GrpcBusEndpoint {
        &self.endpoint
    }

    async fn send_request(&self, path: &str, body: &[u8]) -> BusResult<(u16, Vec<u8>)> {
        let mut stream = TcpStream::connect(format!("{}:{}", self.host, self.port))
            .await
            .map_err(|e| {
                error!("gRPC connect failed: {e}");
                BusError::BusClosed
            })?;

        let request = format!(
            "POST {path} HTTP/1.1\r\n\
             host: {}:{}\r\n\
             content-type: application/x-protobuf\r\n\
             content-length: {}\r\n\
             connection: close\r\n\
             \r\n",
            self.host,
            self.port,
            body.len()
        );

        stream.write_all(request.as_bytes()).await.map_err(|e| {
            error!("gRPC write failed: {e}");
            BusError::BusClosed
        })?;
        stream.write_all(body).await.map_err(|e| {
            error!("gRPC write body failed: {e}");
            BusError::BusClosed
        })?;

        // Read response
        let mut reader = tokio::io::BufReader::new(stream);
        let mut response = String::new();
        let mut buf = [0u8; 1];
        let mut status_code: u16 = 200;
        let mut content_length: usize = 0;

        // Read status line
        loop {
            reader
                .read_exact(&mut buf)
                .await
                .map_err(|_| BusError::BusClosed)?;
            response.push(buf[0] as char);
            if response.ends_with("\r\n") {
                let parts: Vec<&str> = response.trim().split(' ').collect();
                if parts.len() >= 2 {
                    status_code = parts[1].parse().unwrap_or(500);
                }
                break;
            }
        }

        // Read headers
        let mut header_lines = String::new();
        loop {
            reader
                .read_exact(&mut buf)
                .await
                .map_err(|_| BusError::BusClosed)?;
            header_lines.push(buf[0] as char);
            if header_lines.ends_with("\r\n\r\n") {
                for line in header_lines.lines() {
                    if let Some(val) = line.strip_prefix("content-length:") {
                        content_length = val.trim().parse().unwrap_or(0);
                    } else if let Some(val) = line.strip_prefix("Content-Length:") {
                        content_length = val.trim().parse().unwrap_or(0);
                    }
                }
                break;
            }
        }

        // Read body
        let mut body_buf = vec![0u8; content_length];
        if content_length > 0 {
            reader
                .read_exact(&mut body_buf)
                .await
                .map_err(|_| BusError::BusClosed)?;
        }

        Ok((status_code, body_buf))
    }
}

fn parse_address(addr: &str) -> (String, u16) {
    if let Some((host, port_str)) = addr.split_once(':') {
        let port: u16 = port_str.parse().unwrap_or(50051);
        (host.to_string(), port)
    } else {
        (addr.to_string(), 50051)
    }
}

#[async_trait]
impl AgentBusTrait for GrpcBusClient {
    async fn publish(&self, envelope: AgentEnvelope) -> BusResult<String> {
        // Store locally first for drain_for compatibility
        let local_id = {
            let mut msgs = self.messages.lock().await;
            msgs.push(envelope.clone());
            format!("grpc_{}", chrono::Utc::now().timestamp_millis())
        };

        // Try to send over the network
        let proto_env: ProtoAgentEnvelope = (&envelope).into();
        let req_body = PublishRequest {
            envelope: Some(proto_env),
        };
        let mut buf = Vec::with_capacity(req_body.encoded_len());
        if req_body.encode(&mut buf).is_err() {
            return Ok(local_id);
        }

        match self
            .send_request("/agentos.bus.v1.AgentBus/Publish", &buf)
            .await
        {
            Ok((status, body_buf)) => {
                if status == 200 {
                    if let Ok(resp_msg) = PublishResponse::decode(Bytes::from(body_buf)) {
                        if !resp_msg.message_id.is_empty() {
                            return Ok(resp_msg.message_id);
                        }
                    }
                }
                Ok(local_id)
            }
            Err(e) => {
                warn!("gRPC publish network request failed: {e:?}, using local id");
                Ok(local_id)
            }
        }
    }

    async fn drain_for(&self, agent_id: &str) -> Vec<AgentEnvelope> {
        let mut messages = self.messages.lock().await;
        let mut matched = Vec::new();
        messages.retain(|m| {
            if m.target_agent_id == agent_id {
                matched.push(m.clone());
                false
            } else {
                true
            }
        });
        matched
    }

    async fn subscribe(&self, agent_id: &str, topics: &[&str]) {
        let mut subs = self.subscriptions.lock().await;
        subs.entry(agent_id.to_string())
            .or_default()
            .extend(topics.iter().map(|t| t.to_string()));
    }

    async fn unsubscribe(&self, agent_id: &str, topics: &[&str]) {
        let mut subs = self.subscriptions.lock().await;
        if let Some(topics_set) = subs.get_mut(agent_id) {
            topics_set.retain(|t| !topics.contains(&t.as_str()));
        }
    }
}

// ---------------------------------------------------------------------------
// Server-side helpers
// ---------------------------------------------------------------------------

type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

const MAX_PROTOBUF_BODY_BYTES: usize = 4 * 1024 * 1024;

fn json_response(body: &str) -> Response<BoxBody> {
    let mut resp = Response::new(
        Full::new(Bytes::from(body.to_owned()))
            .map_err(|_| unreachable!())
            .boxed(),
    );
    apply_security_headers(&mut resp);
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    resp
}

fn protobuf_response(msg: &impl Message) -> Response<BoxBody> {
    let mut buf = Vec::with_capacity(msg.encoded_len());
    if msg.encode(&mut buf).is_ok() {
        let mut resp = Response::new(
            Full::new(Bytes::from(buf))
                .map_err(|_| unreachable!())
                .boxed(),
        );
        apply_security_headers(&mut resp);
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/x-protobuf"),
        );
        resp
    } else {
        let mut resp = text_response(StatusCode::INTERNAL_SERVER_ERROR, "encode error");
        apply_security_headers(&mut resp);
        resp
    }
}

fn text_response(status: StatusCode, body: &str) -> Response<BoxBody> {
    let mut resp = Response::new(
        Full::new(Bytes::from(body.to_owned()))
            .map_err(|_| unreachable!())
            .boxed(),
    );
    *resp.status_mut() = status;
    apply_security_headers(&mut resp);
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp
}

fn not_found() -> Response<BoxBody> {
    text_response(StatusCode::NOT_FOUND, "not found")
}

fn apply_security_headers(response: &mut Response<BoxBody>) {
    let headers = response.headers_mut();
    headers.insert(
        header::HeaderName::from_static("x-content-type-options"),
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::HeaderName::from_static("x-frame-options"),
        header::HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::HeaderName::from_static("referrer-policy"),
        header::HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::HeaderName::from_static("content-security-policy"),
        header::HeaderValue::from_static(
            "default-src 'none'; frame-ancestors 'none'; form-action 'none'",
        ),
    );
    headers.insert(
        header::HeaderName::from_static("strict-transport-security"),
        header::HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        header::HeaderValue::from_static(
            "camera=(), microphone=(), geolocation=(), payment=(), usb=(), fullscreen=()",
        ),
    );
    headers.insert(
        header::HeaderName::from_static("cross-origin-opener-policy"),
        header::HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        header::HeaderName::from_static("cross-origin-resource-policy"),
        header::HeaderValue::from_static("same-origin"),
    );
}

fn sse_response_headers(response: &mut Response<BoxBody>) {
    apply_security_headers(response);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache, no-store"),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("x-accel-buffering"),
        header::HeaderValue::from_static("no"),
    );
}

async fn collect_limited_body(
    req: Request<Incoming>,
    context: &str,
) -> Result<Bytes, Response<BoxBody>> {
    match Limited::new(req.into_body(), MAX_PROTOBUF_BODY_BYTES)
        .collect()
        .await
    {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(e) => {
            warn!(%context, error = %e, "request body rejected");
            let mut resp = json_response(r#"{"error":"request body too large or invalid"}"#);
            *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
            Err(resp)
        }
    }
}

fn sse_data_frame(data: &str) -> String {
    let normalized = data.replace('\r', "");
    if normalized.is_empty() {
        return "data: \n\n".to_string();
    }

    let mut frame = String::new();
    for line in normalized.lines() {
        frame.push_str("data: ");
        frame.push_str(line);
        frame.push('\n');
    }
    frame.push('\n');
    frame
}

/// A server-sent event destined for SSE clients such as the dashboard.
///
/// When `event` is set, the frame carries an SSE `event:` field so browser
/// clients can route it with `EventSource.addEventListener(name, ...)`.
/// When `event` is `None`, the frame is a plain `data:` message delivered to
/// `EventSource.onmessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

impl SseEvent {
    pub fn named(event: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            event: Some(event.into()),
            data: data.into(),
        }
    }

    pub fn message(data: impl Into<String>) -> Self {
        Self {
            event: None,
            data: data.into(),
        }
    }
}

fn sse_event_frame(event: &SseEvent) -> String {
    let mut frame = String::new();
    if let Some(name) = &event.event {
        // Field values must not contain line breaks; strip them defensively.
        let name = name.replace(['\r', '\n'], "");
        frame.push_str("event: ");
        frame.push_str(&name);
        frame.push('\n');
    }
    frame.push_str(&sse_data_frame(&event.data));
    frame
}

pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    bus: Arc<dyn AgentBusTrait + Send + Sync>,
) {
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("gRPC server failed to bind: {e}");
            return;
        }
    };

    info!("gRPC bus server listening on {}", addr);

    let subscribers: Arc<RwLock<HashMap<String, Vec<tokio::sync::mpsc::Sender<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                warn!("accept error: {e}");
                continue;
            }
        };

        let bus = Arc::clone(&bus);
        let subs = Arc::clone(&subscribers);

        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let conn = hyper::server::conn::http1::Builder::new();
            let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                let bus = Arc::clone(&bus);
                let subs = Arc::clone(&subs);
                async move { handle_grpc_request(req, bus, subs).await }
            });

            if let Err(e) = conn.serve_connection(io, svc).await {
                warn!("gRPC connection error from {peer}: {e}");
            }
        });
    }
}

async fn handle_grpc_request(
    req: Request<Incoming>,
    bus: Arc<dyn AgentBusTrait + Send + Sync>,
    subscribers: Arc<RwLock<HashMap<String, Vec<tokio::sync::mpsc::Sender<String>>>>>,
) -> Result<Response<BoxBody>, hyper::Error> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();

    // Bare liveness stays open; everything that moves messages requires the
    // API token when one is configured.
    if path != "/health" {
        let token = crate::auth::ApiToken::from_env();
        let auth_header = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        if !token.authorize_header(auth_header) {
            return Ok(unauthorized());
        }
    }

    match (method, path.as_str()) {
        (Method::POST, "/agentos.bus.v1.AgentBus/Publish") => {
            handle_publish(req, bus, subscribers).await
        }
        (Method::POST, "/agentos.bus.v1.AgentBus/Subscribe") => {
            handle_subscribe(req, subscribers).await
        }
        (Method::GET, "/health") => Ok(json_response(r#"{"status":"ok"}"#)),
        _ => Ok(not_found()),
    }
}

fn unauthorized() -> Response<BoxBody> {
    let mut resp = text_response(StatusCode::UNAUTHORIZED, "unauthorized");
    resp.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer"),
    );
    resp
}

async fn handle_publish(
    req: Request<Incoming>,
    bus: Arc<dyn AgentBusTrait + Send + Sync>,
    subscribers: Arc<RwLock<HashMap<String, Vec<tokio::sync::mpsc::Sender<String>>>>>,
) -> Result<Response<BoxBody>, hyper::Error> {
    let body_bytes = match collect_limited_body(req, "publish").await {
        Ok(body) => body,
        Err(resp) => return Ok(resp),
    };

    let publish_req = match PublishRequest::decode(body_bytes) {
        Ok(msg) => msg,
        Err(e) => {
            error!("failed to decode PublishRequest: {e}");
            let mut resp = json_response(r#"{"error":"failed to decode request"}"#);
            *resp.status_mut() = StatusCode::BAD_REQUEST;
            return Ok(resp);
        }
    };

    let proto_env = match publish_req.envelope {
        Some(e) => e,
        None => {
            let mut resp = json_response(r#"{"error":"missing envelope"}"#);
            *resp.status_mut() = StatusCode::BAD_REQUEST;
            return Ok(resp);
        }
    };

    let envelope: AgentEnvelope = proto_env.into();
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let id = format!("grpc_{}_{}", envelope.source_agent_id, now);

    // Publish the envelope onto the bus
    let _ = bus.publish(envelope.clone()).await;

    // Fan out to subscribers
    {
        let subs = subscribers.read().await;
        for senders in subs.values() {
            for tx in senders {
                let _ = tx.send(id.clone()).await;
            }
        }
    }

    let resp_msg = PublishResponse {
        message_id: id.clone(),
    };
    let mut resp = protobuf_response(&resp_msg);
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/x-protobuf"),
    );
    Ok(resp)
}

async fn handle_subscribe(
    req: Request<Incoming>,
    subscribers: Arc<RwLock<HashMap<String, Vec<mpsc::Sender<String>>>>>,
) -> Result<Response<BoxBody>, hyper::Error> {
    let body_bytes = match collect_limited_body(req, "subscribe").await {
        Ok(body) => body,
        Err(resp) => return Ok(resp),
    };

    let sub_req = match SubscribeRequest::decode(body_bytes) {
        Ok(msg) => msg,
        Err(e) => {
            error!("failed to decode SubscribeRequest: {e}");
            let mut resp = json_response(r#"{"error":"failed to decode request"}"#);
            *resp.status_mut() = StatusCode::BAD_REQUEST;
            return Ok(resp);
        }
    };

    let agent_id = if sub_req.agent_id.is_empty() {
        "anonymous"
    } else {
        &sub_req.agent_id
    };

    let (tx, rx) = mpsc::channel::<String>(64);

    {
        let mut subs = subscribers.write().await;
        subs.entry(agent_id.to_string()).or_default().push(tx);
    }

    info!(agent = %agent_id, topics = ?sub_req.topics, "gRPC client subscribed");

    let stream = ReceiverStream::new(rx)
        .map(|msg| Ok::<_, hyper::Error>(Frame::data(Bytes::from(sse_data_frame(&msg)))));

    let mut resp = Response::new(StreamBody::new(stream).map_err(|e| e).boxed());
    sse_response_headers(&mut resp);
    Ok(resp)
}

// ---------------------------------------------------------------------------
// SSE endpoint – real‑time event stream for dashboard / CLI
// ---------------------------------------------------------------------------

/// Spawns an SSE endpoint on the given address that fans out every event
/// received on `event_rx` to all connected SSE clients (dashboard, CLI).
pub async fn start_sse_server(
    addr: std::net::SocketAddr,
    event_rx: tokio::sync::broadcast::Receiver<SseEvent>,
) {
    use std::convert::Infallible;

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("SSE server failed to bind: {e}");
            return;
        }
    };

    info!("SSE event stream listening on http://{addr}/events");

    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<SseEvent>(1024);

    // Forward from the system event bus to the broadcast channel
    let tx = broadcast_tx.clone();
    tokio::spawn(async move {
        let mut rx = event_rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let _ = tx.send(event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("SSE broadcast lagged by {n} messages");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                warn!("SSE accept error: {e}");
                continue;
            }
        };

        let rx = broadcast_tx.subscribe();

        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                let rx = rx.resubscribe();
                async move {
                    match (req.method(), req.uri().path()) {
                        (&Method::GET, "/events") => {
                            // EventSource cannot set headers, so the SSE
                            // surface accepts the token as a query param too.
                            let token = crate::auth::ApiToken::from_env();
                            let auth_header = req
                                .headers()
                                .get(header::AUTHORIZATION)
                                .and_then(|v| v.to_str().ok());
                            if !token.authorize_header_or_query(auth_header, req.uri().query()) {
                                return Ok::<_, Infallible>(unauthorized());
                            }
                            Ok::<_, Infallible>(sse_handler(rx).await)
                        }
                        _ => Ok(not_found()),
                    }
                }
            });

            let conn = hyper::server::conn::http1::Builder::new();
            if let Err(e) = conn.serve_connection(io, svc).await {
                warn!("SSE connection error from {peer}: {e}");
            }
        });
    }
}

async fn sse_handler(rx: tokio::sync::broadcast::Receiver<SseEvent>) -> Response<BoxBody> {
    use futures::StreamExt;

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(|msg| {
        let event = match msg {
            Ok(m) => m,
            Err(_) => return Ok(Frame::data(Bytes::from(sse_data_frame("{}")))),
        };
        Ok::<_, hyper::Error>(Frame::data(Bytes::from(sse_event_frame(&event))))
    });

    let body = StreamBody::new(stream).map_err(|e| e).boxed();
    let mut resp = Response::new(body);
    sse_response_headers(&mut resp);
    resp
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_client_publish_fallback_local() {
        // Even with no server, publish falls back to local id
        let ep = GrpcBusEndpoint::new("127.0.0.1:1");
        let client = GrpcBusClient::new(ep);
        let env = AgentEnvelope::new("alice", "bob", "test", vec![]);
        let id = client.publish(env).await.unwrap();
        assert!(id.starts_with("grpc_"));

        // Message is stored locally for drain_for
        let drained = client.drain_for("bob").await;
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].source_agent_id, "alice");
    }

    #[test]
    fn test_grpc_endpoint_describe() {
        let ep = GrpcBusEndpoint::new("localhost:50051");
        assert_eq!(ep.describe(), "http://localhost:50051");
        let ep_tls = GrpcBusEndpoint::new("localhost:50051").with_tls(true);
        assert_eq!(ep_tls.describe(), "https://localhost:50051");
    }

    #[test]
    fn test_sse_event_frame_named() {
        let frame = sse_event_frame(&SseEvent::named("agent_started", r#"{"id":"a1"}"#));
        assert_eq!(frame, "event: agent_started\ndata: {\"id\":\"a1\"}\n\n");
    }

    #[test]
    fn test_sse_event_frame_unnamed_message() {
        let frame = sse_event_frame(&SseEvent::message("hello"));
        assert_eq!(frame, "data: hello\n\n");
    }

    #[test]
    fn test_sse_event_frame_multiline_data() {
        let frame = sse_event_frame(&SseEvent::named("agent_event", "line1\nline2"));
        assert_eq!(frame, "event: agent_event\ndata: line1\ndata: line2\n\n");
    }

    #[test]
    fn test_sse_event_frame_strips_newlines_from_event_name() {
        let frame = sse_event_frame(&SseEvent::named("bad\nname", "x"));
        assert_eq!(frame, "event: badname\ndata: x\n\n");
    }

    #[test]
    fn test_json_response_uses_secure_headers() {
        let resp = json_response(r#"{"status":"ok"}"#);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json; charset=utf-8"
        );
        assert_eq!(
            resp.headers()
                .get(header::HeaderName::from_static("x-content-type-options"))
                .unwrap(),
            "nosniff"
        );
        assert_eq!(
            resp.headers()
                .get(header::HeaderName::from_static("x-frame-options"))
                .unwrap(),
            "DENY"
        );
    }

    #[test]
    fn test_sse_data_frame_prefixes_every_line() {
        assert_eq!(
            sse_data_frame("hello\nid: injected\nevent: close\r"),
            "data: hello\ndata: id: injected\ndata: event: close\n\n"
        );
    }

    #[test]
    fn test_proto_envelope_roundtrip() {
        let env = AgentEnvelope {
            id: "test_id".into(),
            source_agent_id: "alice".into(),
            target_agent_id: "bob".into(),
            topic: "test.topic".into(),
            payload: vec![1, 2, 3],
            timestamp_ms: 12345,
        };
        let proto: ProtoAgentEnvelope = (&env).into();
        let decoded: AgentEnvelope = proto.into();
        assert_eq!(env.id, decoded.id);
        assert_eq!(env.source_agent_id, decoded.source_agent_id);
        assert_eq!(env.payload, decoded.payload);
        assert_eq!(env.timestamp_ms, decoded.timestamp_ms);
    }

    #[test]
    fn test_publish_request_encode_decode() {
        let envelope = ProtoAgentEnvelope {
            id: "msg1".into(),
            source_agent_id: "alice".into(),
            target_agent_id: "bob".into(),
            topic: "chat".into(),
            payload: vec![0xAB; 16],
            timestamp_ms: 999,
        };
        let req = PublishRequest {
            envelope: Some(envelope.clone()),
        };
        let mut buf = Vec::with_capacity(req.encoded_len());
        req.encode(&mut buf).unwrap();

        let decoded = PublishRequest::decode(Bytes::from(buf)).unwrap();
        let decoded_env = decoded.envelope.unwrap();
        assert_eq!(decoded_env.id, envelope.id);
        assert_eq!(decoded_env.payload, envelope.payload);
    }

    #[test]
    fn test_subscribe_request_encode_decode() {
        let req = SubscribeRequest {
            agent_id: "agent-1".into(),
            topics: vec!["broadcast".into(), "alerts".into()],
        };
        let mut buf = Vec::with_capacity(req.encoded_len());
        req.encode(&mut buf).unwrap();

        let decoded = SubscribeRequest::decode(Bytes::from(buf)).unwrap();
        assert_eq!(decoded.agent_id, "agent-1");
        assert_eq!(decoded.topics.len(), 2);
    }
}
