//! Prometheus metrics endpoint for AgentOS.
//!
//! Exposes agent runtime metrics in Prometheus text format at `/metrics`.
//! Metrics include agent counts, event counts, memory usage, and operational stats.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{error, info};

/// Collects and exposes Prometheus metrics for the agent runtime.
#[derive(Debug, Default)]
pub struct MetricsCollector {
    pub agents_spawned: AtomicU64,
    pub agents_stopped: AtomicU64,
    pub agents_failed: AtomicU64,
    pub messages_published: AtomicU64,
    pub messages_consumed: AtomicU64,
    pub tools_called: AtomicU64,
    pub llm_requests: AtomicU64,
    pub llm_errors: AtomicU64,
    pub memory_queries: AtomicU64,
    pub active_agents: AtomicU64,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_spawned(&self) {
        self.agents_spawned.fetch_add(1, Ordering::Relaxed);
        self.active_agents.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_stopped(&self) {
        self.agents_stopped.fetch_add(1, Ordering::Relaxed);
        self.active_agents.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn inc_failed(&self) {
        self.agents_failed.fetch_add(1, Ordering::Relaxed);
        self.active_agents.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn inc_published(&self) {
        self.messages_published.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_consumed(&self) {
        self.messages_consumed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_tool_call(&self) {
        self.tools_called.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_llm_request(&self) {
        self.llm_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_llm_error(&self) {
        self.llm_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_memory_query(&self) {
        self.memory_queries.fetch_add(1, Ordering::Relaxed);
    }

    /// Render metrics in Prometheus text format.
    pub fn render(&self, extra: Option<&HashMap<String, u64>>) -> String {
        let mut output = String::new();

        output.push_str("# HELP agentos_agents_spawned Total agents spawned\n");
        output.push_str("# TYPE agentos_agents_spawned counter\n");
        output.push_str(&format!(
            "agentos_agents_spawned {}\n",
            self.agents_spawned.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_agents_stopped Total agents stopped\n");
        output.push_str("# TYPE agentos_agents_stopped counter\n");
        output.push_str(&format!(
            "agentos_agents_stopped {}\n",
            self.agents_stopped.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_agents_failed Total agents failed\n");
        output.push_str("# TYPE agentos_agents_failed counter\n");
        output.push_str(&format!(
            "agentos_agents_failed {}\n",
            self.agents_failed.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_active_agents Currently active agents\n");
        output.push_str("# TYPE agentos_active_agents gauge\n");
        output.push_str(&format!(
            "agentos_active_agents {}\n",
            self.active_agents.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_messages_published Total messages published\n");
        output.push_str("# TYPE agentos_messages_published counter\n");
        output.push_str(&format!(
            "agentos_messages_published {}\n",
            self.messages_published.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_messages_consumed Total messages consumed\n");
        output.push_str("# TYPE agentos_messages_consumed counter\n");
        output.push_str(&format!(
            "agentos_messages_consumed {}\n",
            self.messages_consumed.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_tools_called Total tool invocations\n");
        output.push_str("# TYPE agentos_tools_called counter\n");
        output.push_str(&format!(
            "agentos_tools_called {}\n",
            self.tools_called.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_llm_requests Total LLM API requests\n");
        output.push_str("# TYPE agentos_llm_requests counter\n");
        output.push_str(&format!(
            "agentos_llm_requests {}\n",
            self.llm_requests.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_llm_errors Total LLM API errors\n");
        output.push_str("# TYPE agentos_llm_errors counter\n");
        output.push_str(&format!(
            "agentos_llm_errors {}\n",
            self.llm_errors.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP agentos_memory_queries Total memory searches\n");
        output.push_str("# TYPE agentos_memory_queries counter\n");
        output.push_str(&format!(
            "agentos_memory_queries {}\n",
            self.memory_queries.load(Ordering::Relaxed)
        ));

        if let Some(extra) = extra {
            for (key, value) in extra {
                let safe_key = key.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                output.push_str(&format!("# HELP agentos_{0} Custom metric\n", safe_key));
                output.push_str(&format!("# TYPE agentos_{0} gauge\n", safe_key));
                output.push_str(&format!("agentos_{0} {1}\n", safe_key, value));
            }
        }

        output
    }
}

/// Start a Prometheus metrics HTTP server on the given address.
pub async fn start_metrics_server(
    addr: SocketAddr,
    collector: Arc<MetricsCollector>,
) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(addr).await?;
    info!(address = %addr, "Prometheus metrics server started");

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let collector = collector.clone();
                tokio::spawn(async move {
                    let service =
                        hyper::service::service_fn(move |req: Request<hyper::body::Incoming>| {
                            let collector = collector.clone();
                            async move {
                                if req.uri().path() == "/metrics" || req.uri().path() == "/" {
                                    let body = collector.render(None);
                                    Ok::<_, hyper::Error>(
                                        Response::builder()
                                            .status(StatusCode::OK)
                                            .header("Content-Type", "text/plain; version=0.0.4")
                                            .body(Full::new(Bytes::from(body)))
                                            .unwrap(),
                                    )
                                } else {
                                    Ok(Response::builder()
                                        .status(StatusCode::NOT_FOUND)
                                        .body(Full::new(Bytes::from("not found")))
                                        .unwrap())
                                }
                            }
                        });

                    let io = TokioIo::new(stream);
                    if let Err(e) = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service)
                        .await
                    {
                        error!(peer = %peer, error = %e, "metrics connection error");
                    }
                });
            }
            Err(e) => {
                error!("Metrics server accept error: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_defaults() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.agents_spawned.load(Ordering::Relaxed), 0);
        assert_eq!(collector.active_agents.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_increment() {
        let collector = MetricsCollector::new();
        collector.inc_spawned();
        collector.inc_spawned();
        collector.inc_stopped();
        assert_eq!(collector.agents_spawned.load(Ordering::Relaxed), 2);
        assert_eq!(collector.active_agents.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_metrics_render() {
        let collector = MetricsCollector::new();
        collector.inc_spawned();
        collector.inc_published();
        let output = collector.render(None);
        assert!(output.contains("agentos_agents_spawned 1"));
        assert!(output.contains("agentos_messages_published 1"));
        assert!(output.contains("agentos_active_agents 1"));
    }

    #[test]
    fn test_metrics_render_with_extra() {
        let collector = MetricsCollector::new();
        let mut extra = HashMap::new();
        extra.insert("cpu_usage".into(), 42);
        let output = collector.render(Some(&extra));
        assert!(output.contains("agentos_cpu_usage"));
        assert!(output.contains("agentos_cpu_usage 42"));
    }
}
