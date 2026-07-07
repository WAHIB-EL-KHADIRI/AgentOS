use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{header, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::system::AgentOSSystem;

const MAX_REQUEST_BODY_BYTES: u64 = 1024 * 1024; // 1 MB

pub type MetricsRegistry = Arc<RwLock<HashMap<String, u64>>>;

#[derive(Clone)]
pub struct HealthServer {
    system: Arc<AgentOSSystem>,
    metrics: MetricsRegistry,
    addr: SocketAddr,
}

impl HealthServer {
    pub fn new(system: Arc<AgentOSSystem>, addr: SocketAddr) -> Self {
        Self {
            system,
            metrics: Arc::new(RwLock::new(HashMap::new())),
            addr,
        }
    }

    pub fn metrics_registry(&self) -> MetricsRegistry {
        Arc::clone(&self.metrics)
    }

    pub async fn increment_counter(&self, name: &str) {
        let mut metrics = self.metrics.write().await;
        *metrics.entry(name.to_string()).or_insert(0) += 1;
    }

    pub async fn start(self) {
        let listener = match TcpListener::bind(self.addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("health server failed to bind: {e}");
                return;
            }
        };

        info!("health server listening on {}", self.addr);

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    warn!("accept error: {e}");
                    continue;
                }
            };

            let this = self.clone();
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let svc = service_fn(move |req: Request<Incoming>| {
                    let this = this.clone();
                    async move { this.handle_request(req).await }
                });

                let conn = hyper::server::conn::http1::Builder::new();
                if let Err(e) = conn.serve_connection(io, svc).await {
                    warn!("connection error from {peer}: {e}");
                }
            });
        }
    }

    async fn handle_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<String>, std::convert::Infallible> {
        // Enforce request body size limit
        let content_length = req
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        if content_length > MAX_REQUEST_BODY_BYTES {
            return Ok(text_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body too large",
            ));
        }

        let path = req.uri().path().to_string();
        let method = req.method().clone();

        match (method, path.as_str()) {
            (Method::GET, "/health") => Ok(self.health_response()),
            (Method::GET, "/health/live") => Ok(self.liveness_response()),
            (Method::GET, "/health/ready") => Ok(self.readiness_response().await),
            (Method::GET, "/metrics") => Ok(self.metrics_response().await),
            (Method::GET, "/api/v1/agents") => Ok(self.list_agents_response().await),
            (Method::GET, path) if path.starts_with("/api/v1/agents/") => {
                let agent_id = path.trim_start_matches("/api/v1/agents/");
                if !is_valid_agent_id(agent_id) {
                    Ok(json_response(
                        StatusCode::BAD_REQUEST,
                        serde_json::json!({"error":"invalid agent id"}),
                    ))
                } else {
                    Ok(self.agent_detail_response(agent_id).await)
                }
            }
            _ => Ok(text_response(StatusCode::NOT_FOUND, "not found")),
        }
    }

    fn health_response(&self) -> Response<String> {
        json_response(
            StatusCode::OK,
            serde_json::json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
                "service": "agentOS"
            }),
        )
    }

    fn liveness_response(&self) -> Response<String> {
        json_response(StatusCode::OK, serde_json::json!({"status": "alive"}))
    }

    async fn readiness_response(&self) -> Response<String> {
        let agents = self.system.supervisor.list().await;
        let mut running = 0usize;
        for h in &agents {
            if h.state().await == crate::agent::AgentState::Running {
                running += 1;
            }
        }
        json_response(
            StatusCode::OK,
            serde_json::json!({
                "status": "ready",
                "agents_running": running,
                "total_agents": agents.len()
            }),
        )
    }

    async fn metrics_response(&self) -> Response<String> {
        let metrics = self.metrics.read().await;
        json_response(StatusCode::OK, serde_json::json!(*metrics))
    }

    async fn list_agents_response(&self) -> Response<String> {
        let agents = self.system.supervisor.list().await;
        let mut agent_list = Vec::new();
        for h in &agents {
            agent_list.push(serde_json::json!({
                "id": h.id,
                "state": format!("{:?}", h.state().await),
                "restarts": h.restart_count(),
                "last_heartbeat": h.last_heartbeat(),
            }));
        }

        json_response(StatusCode::OK, serde_json::json!(agent_list))
    }

    async fn agent_detail_response(&self, agent_id: &str) -> Response<String> {
        match self.system.supervisor.get(agent_id).await {
            Some(handle) => {
                let logs = self.system.get_logs(agent_id, 50).await;
                let logs_json: Vec<serde_json::Value> = logs
                    .into_iter()
                    .map(|entry| {
                        serde_json::json!({
                            "timestamp_ms": entry.timestamp_ms,
                            "event_type": entry.event_type,
                            "message": entry.message,
                        })
                    })
                    .collect();
                json_response(
                    StatusCode::OK,
                    serde_json::json!({
                        "id": handle.id,
                        "state": format!("{:?}", handle.state().await),
                        "restarts": handle.restart_count(),
                        "last_heartbeat": handle.last_heartbeat(),
                        "logs": logs_json,
                    }),
                )
            }
            None => json_response(
                StatusCode::NOT_FOUND,
                serde_json::json!({
                    "error": format!("agent '{agent_id}' not found")
                }),
            ),
        }
    }
}

fn json_response(status: StatusCode, value: serde_json::Value) -> Response<String> {
    let mut response = Response::new(value.to_string());
    *response.status_mut() = status;
    apply_security_headers(&mut response);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn text_response(status: StatusCode, body: &str) -> Response<String> {
    let mut response = Response::new(body.to_string());
    *response.status_mut() = status;
    apply_security_headers(&mut response);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn apply_security_headers(response: &mut Response<String>) {
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
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
    headers.insert(
        header::HeaderName::from_static("x-dns-prefetch-control"),
        header::HeaderValue::from_static("off"),
    );
}

fn is_valid_agent_id(agent_id: &str) -> bool {
    !agent_id.is_empty()
        && agent_id.len() <= 128
        && agent_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::AgentOSSystem;

    #[tokio::test]
    async fn test_health_response() {
        let system = Arc::new(AgentOSSystem::new());
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = HealthServer::new(system, addr);

        let resp = server.health_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_str(resp.body()).unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "agentOS");
    }

    #[tokio::test]
    async fn test_liveness_response() {
        let system = Arc::new(AgentOSSystem::new());
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = HealthServer::new(system, addr);

        let resp = server.liveness_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_str(resp.body()).unwrap();
        assert_eq!(body["status"], "alive");
    }

    #[test]
    fn test_json_response_uses_secure_headers() {
        let resp = json_response(StatusCode::OK, serde_json::json!({"status":"ok"}));
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
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }

    #[test]
    fn test_agent_id_validation_rejects_path_like_values() {
        assert!(is_valid_agent_id("agent-1_ok.foo:bar"));
        assert!(!is_valid_agent_id(""));
        assert!(!is_valid_agent_id("../agent"));
        assert!(!is_valid_agent_id("agent/child"));
        assert!(!is_valid_agent_id("agent\nchild"));
    }

    #[tokio::test]
    async fn test_metrics_increment() {
        let system = Arc::new(AgentOSSystem::new());
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = HealthServer::new(system, addr);

        server.increment_counter("requests").await;
        server.increment_counter("requests").await;

        let metrics = server.metrics.read().await;
        assert_eq!(metrics.get("requests"), Some(&2));
    }
}
