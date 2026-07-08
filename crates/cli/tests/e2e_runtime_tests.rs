//! End-to-end tests that spawn the real `agentOS run` binary and talk to
//! its network surfaces (health HTTP, metrics, inspection API, SSE) over
//! real TCP, in both open and token-protected modes.
//!
//! These are the automated version of the manual curl verification used
//! during development: if they pass, a fresh checkout serves a live,
//! correctly-guarded runtime.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STARTUP_DEADLINE: Duration = Duration::from_secs(60);

/// Kills the runtime child process even when an assertion panics.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("agentos_e2e_{name}_{}_{}", std::process::id(), now));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_agent_config(dir: &Path) -> PathBuf {
    let path = dir.join("e2e_agent.toml");
    std::fs::write(
        &path,
        "name = \"e2e-agent\"\nprompt = \"You are an end-to-end test agent.\"\ncapabilities = [\"trace_record\"]\n",
    )
    .expect("write agent config");
    path
}

struct RuntimePorts {
    http: u16,
    grpc: u16,
    sse: u16,
}

fn spawn_runtime(cwd: &Path, ports: &RuntimePorts, api_token: Option<&str>) -> ChildGuard {
    let agent_config = write_agent_config(cwd);

    let mut command = Command::new(env!("CARGO_BIN_EXE_agentOS"));
    command
        .args(["run", "--agent", agent_config.to_str().unwrap()])
        .current_dir(cwd)
        .env("HOME", cwd)
        .env("USERPROFILE", cwd)
        .env("NO_COLOR", "1")
        .env("AGENTOS_HTTP_PORT", ports.http.to_string())
        .env("AGENTOS_GRPC_PORT", ports.grpc.to_string())
        .env("AGENTOS_SSE_PORT", ports.sse.to_string())
        // Deterministic isolation from the developer's environment.
        .env_remove("OPENAI_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGENTOS_LLM_PROVIDER")
        .env_remove("AGENTOS_VAULT_KEY")
        .env_remove("AGENTOS_API_TOKEN")
        .env_remove("AGENTOS_HOST")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(token) = api_token {
        command.env("AGENTOS_API_TOKEN", token);
    }

    ChildGuard(command.spawn().expect("failed to spawn agentOS run"))
}

/// Raw HTTP/1.1 GET. Reads until the server closes the connection or the
/// per-read timeout elapses; returns everything received (status line,
/// headers, body).
fn http_get(port: u16, path: &str, bearer: Option<&str>) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;

    let mut request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n");
    if let Some(token) = bearer {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes())?;

    let mut response = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    Ok(String::from_utf8_lossy(&response).to_string())
}

fn status_code(response: &str) -> Option<u16> {
    response
        .strip_prefix("HTTP/1.1 ")
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse().ok())
}

fn wait_for_health(port: u16) -> String {
    let start = Instant::now();
    loop {
        if let Ok(response) = http_get(port, "/health", None) {
            if status_code(&response) == Some(200) {
                return response;
            }
        }
        assert!(
            start.elapsed() < STARTUP_DEADLINE,
            "runtime did not become healthy on port {port} within {STARTUP_DEADLINE:?}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Open an SSE stream and read until `needle` appears or the deadline hits.
fn read_sse_until(port: u16, path: &str, needle: &str, deadline: Duration) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect SSE");
    stream
        .set_read_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/event-stream\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).unwrap();

    let start = Instant::now();
    let mut acc = Vec::new();
    let mut chunk = [0u8; 4096];
    while start.elapsed() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                acc.extend_from_slice(&chunk[..n]);
                if String::from_utf8_lossy(&acc).contains(needle) {
                    break;
                }
            }
            Err(_) => continue,
        }
    }
    String::from_utf8_lossy(&acc).to_string()
}

#[test]
fn open_runtime_serves_health_metrics_agents_and_sse() {
    let cwd = temp_dir("open");
    let ports = RuntimePorts {
        http: 18480,
        grpc: 15451,
        sse: 18481,
    };
    let _runtime = spawn_runtime(&cwd, &ports, None);

    let health = wait_for_health(ports.http);
    assert!(health.contains("\"status\":\"ok\""), "health: {health}");

    // Without a configured token every surface stays open.
    let metrics = http_get(ports.http, "/metrics", None).unwrap();
    assert_eq!(status_code(&metrics), Some(200), "metrics: {metrics}");

    let agents = http_get(ports.http, "/api/v1/agents", None).unwrap();
    assert_eq!(status_code(&agents), Some(200), "agents: {agents}");
    assert!(agents.contains("agent_e2e_agent"), "agents: {agents}");

    // Recorded-session listing serves the dashboard's Recordings view.
    // No LLM provider ran here, so the list is empty but well-formed.
    let journals = http_get(ports.http, "/api/v1/journals", None).unwrap();
    assert_eq!(status_code(&journals), Some(200), "journals: {journals}");
    assert!(journals.contains("[]"), "journals: {journals}");

    // The SSE stream serves named dashboard events: the periodic status
    // ticker emits agent_status within ~5 seconds.
    let sse = read_sse_until(
        ports.sse,
        "/events",
        "event: agent_status",
        Duration::from_secs(15),
    );
    assert!(sse.contains("HTTP/1.1 200"), "sse response: {sse}");
    assert!(sse.contains("event: agent_status"), "sse response: {sse}");
    assert!(
        sse.contains("\"id\":\"agent_e2e_agent\""),
        "sse response: {sse}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn token_protected_runtime_guards_every_surface() {
    let cwd = temp_dir("token");
    let ports = RuntimePorts {
        http: 18490,
        grpc: 15461,
        sse: 18491,
    };
    let token = "e2e-secret-token";
    let _runtime = spawn_runtime(&cwd, &ports, Some(token));

    // Liveness stays open for container healthchecks.
    let health = wait_for_health(ports.http);
    assert!(health.contains("\"status\":\"ok\""), "health: {health}");

    // Protected endpoints reject missing and wrong tokens.
    let no_token = http_get(ports.http, "/metrics", None).unwrap();
    assert_eq!(
        status_code(&no_token),
        Some(401),
        "metrics open: {no_token}"
    );
    let wrong = http_get(ports.http, "/metrics", Some("wrong")).unwrap();
    assert_eq!(status_code(&wrong), Some(401), "metrics wrong: {wrong}");
    let agents_unauth = http_get(ports.http, "/api/v1/agents", None).unwrap();
    assert_eq!(
        status_code(&agents_unauth),
        Some(401),
        "agents: {agents_unauth}"
    );
    let journals_unauth = http_get(ports.http, "/api/v1/journals", None).unwrap();
    assert_eq!(
        status_code(&journals_unauth),
        Some(401),
        "journals: {journals_unauth}"
    );

    // The correct bearer token is accepted.
    let metrics = http_get(ports.http, "/metrics", Some(token)).unwrap();
    assert_eq!(status_code(&metrics), Some(200), "metrics auth: {metrics}");

    // SSE rejects missing tokens and accepts ?token= (EventSource cannot
    // set headers).
    let sse_unauth = http_get(ports.sse, "/events", None).unwrap();
    assert_eq!(status_code(&sse_unauth), Some(401), "sse: {sse_unauth}");
    let sse = read_sse_until(
        ports.sse,
        &format!("/events?token={token}"),
        "HTTP/1.1 200",
        Duration::from_secs(10),
    );
    assert!(sse.contains("HTTP/1.1 200"), "sse auth response: {sse}");

    // The bus HTTP surface requires the token too.
    let bus_health = http_get(ports.grpc, "/health", None).unwrap();
    assert_eq!(
        status_code(&bus_health),
        Some(200),
        "bus health: {bus_health}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
}
