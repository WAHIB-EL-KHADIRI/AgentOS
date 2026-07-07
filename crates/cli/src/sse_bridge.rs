//! Bridges kernel system events onto the SSE broadcast channel using the
//! payload shapes the dashboard expects (see `dashboard/src/types.ts`).
//!
//! The dashboard subscribes with `EventSource.addEventListener(name, ...)`,
//! so every frame emitted here carries a named SSE `event:` field.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use agentos_bus::SseEvent;
use agentos_kernel::events::{EventListener, SystemEvent, SystemEventType};
use agentos_kernel::AgentState;
use tokio::sync::broadcast;

/// Forwards kernel system events to SSE clients as dashboard events.
///
/// Constructed for the single-agent `agentOS run` flow: the bridge knows the
/// running agent's name and capabilities so it can render full `AgentInfo`
/// payloads, which `SystemEvent` alone does not carry.
pub struct DashboardSseBridge {
    tx: broadcast::Sender<SseEvent>,
    agent_id: String,
    agent_name: String,
    capabilities: Vec<String>,
    started_at_ms: u64,
    trace_count: Arc<AtomicU64>,
}

impl DashboardSseBridge {
    pub fn new(
        tx: broadcast::Sender<SseEvent>,
        agent_id: impl Into<String>,
        agent_name: impl Into<String>,
        capabilities: Vec<String>,
        started_at_ms: u64,
    ) -> Self {
        Self {
            tx,
            agent_id: agent_id.into(),
            agent_name: agent_name.into(),
            capabilities,
            started_at_ms,
            trace_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Shared trace counter, so other emitters (e.g. the periodic status
    /// ticker) can report a consistent `trace_count`.
    pub fn trace_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.trace_count)
    }

    fn send(&self, event: SseEvent) {
        // No receivers connected yet is normal for a live stream.
        let _ = self.tx.send(event);
    }

    fn agent_info_json(&self, status: &str) -> String {
        agent_info_json(
            &self.agent_id,
            &self.agent_name,
            status,
            &self.capabilities,
            self.started_at_ms,
            self.trace_count.load(Ordering::Relaxed),
        )
    }

    fn display_name(&self, agent_id: &str) -> String {
        if agent_id == self.agent_id {
            self.agent_name.clone()
        } else {
            agent_id.to_string()
        }
    }
}

impl EventListener for DashboardSseBridge {
    fn on_event(&self, event: &SystemEvent) {
        let agent_id = event.agent_id.as_deref().unwrap_or("");

        match &event.event_type {
            SystemEventType::AgentSpawned => {
                self.send(SseEvent::named(
                    "agent_started",
                    self.agent_info_json("running"),
                ));
            }
            SystemEventType::AgentStopped | SystemEventType::AgentFailed => {
                self.send(SseEvent::named(
                    "agent_stopped",
                    serde_json::json!({ "agent_id": agent_id }).to_string(),
                ));
            }
            SystemEventType::AgentDegraded => {
                self.send(SseEvent::named(
                    "agent_status",
                    self.agent_info_json("error"),
                ));
            }
            SystemEventType::ThoughtRecorded => {
                self.trace_count.fetch_add(1, Ordering::Relaxed);
                self.send(SseEvent::named(
                    "trace_update",
                    serde_json::json!({
                        "checkpoint_id": event.id,
                        "label": event.payload,
                        "agent_id": agent_id,
                        "status": "complete",
                        "timestamp": iso_timestamp(event.timestamp_ms),
                    })
                    .to_string(),
                ));
            }
            _ => {}
        }

        // Every system event also lands in the dashboard's event stream.
        self.send(SseEvent::named(
            "agent_event",
            serde_json::json!({
                "id": event.id,
                "agent_id": agent_id,
                "agent_name": self.display_name(agent_id),
                "event_type": dashboard_event_type(&event.event_type),
                "payload": event.payload,
                "timestamp": iso_timestamp(event.timestamp_ms),
            })
            .to_string(),
        ));
    }
}

/// Render an `AgentInfo` payload (dashboard shape) as JSON.
pub fn agent_info_json(
    agent_id: &str,
    agent_name: &str,
    status: &str,
    capabilities: &[String],
    started_at_ms: u64,
    trace_count: u64,
) -> String {
    serde_json::json!({
        "id": agent_id,
        "name": agent_name,
        "status": status,
        "capabilities": capabilities,
        "started_at": iso_timestamp(started_at_ms),
        "trace_count": trace_count,
    })
    .to_string()
}

/// Map a kernel agent state onto the dashboard's `AgentStatus` union:
/// `running | stopped | error | starting`.
pub fn agent_status_label(state: &AgentState) -> &'static str {
    match state {
        AgentState::Running => "running",
        AgentState::Created => "starting",
        AgentState::Stopped => "stopped",
        AgentState::Degraded(_) | AgentState::Failed(_) => "error",
    }
}

/// Map kernel event types onto the dashboard's `EventType` union:
/// `thought | tool_call | tool_result | error | state_change | message`.
fn dashboard_event_type(event_type: &SystemEventType) -> &'static str {
    match event_type {
        SystemEventType::ThoughtRecorded => "thought",
        SystemEventType::AgentFailed => "error",
        SystemEventType::AgentSpawned
        | SystemEventType::AgentStopped
        | SystemEventType::AgentDegraded => "state_change",
        _ => "message",
    }
}

fn iso_timestamp(ms: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms as i64)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event(event_type: SystemEventType) -> SystemEvent {
        SystemEvent {
            id: "evt_1".into(),
            event_type,
            agent_id: Some("agent_demo".into()),
            payload: "payload text".into(),
            timestamp_ms: 1_700_000_000_000,
            sequence: 1,
        }
    }

    fn bridge_with_channel() -> (DashboardSseBridge, broadcast::Receiver<SseEvent>) {
        let (tx, rx) = broadcast::channel(16);
        let bridge = DashboardSseBridge::new(
            tx,
            "agent_demo",
            "Demo Agent",
            vec!["memory".into()],
            1_700_000_000_000,
        );
        (bridge, rx)
    }

    #[test]
    fn test_spawn_maps_to_agent_started() {
        let (bridge, mut rx) = bridge_with_channel();
        bridge.on_event(&sample_event(SystemEventType::AgentSpawned));

        let first = rx.try_recv().unwrap();
        assert_eq!(first.event.as_deref(), Some("agent_started"));
        let info: serde_json::Value = serde_json::from_str(&first.data).unwrap();
        assert_eq!(info["id"], "agent_demo");
        assert_eq!(info["name"], "Demo Agent");
        assert_eq!(info["status"], "running");

        let second = rx.try_recv().unwrap();
        assert_eq!(second.event.as_deref(), Some("agent_event"));
    }

    #[test]
    fn test_stop_and_fail_map_to_agent_stopped() {
        for event_type in [SystemEventType::AgentStopped, SystemEventType::AgentFailed] {
            let (bridge, mut rx) = bridge_with_channel();
            bridge.on_event(&sample_event(event_type));
            let first = rx.try_recv().unwrap();
            assert_eq!(first.event.as_deref(), Some("agent_stopped"));
            let payload: serde_json::Value = serde_json::from_str(&first.data).unwrap();
            assert_eq!(payload["agent_id"], "agent_demo");
        }
    }

    #[test]
    fn test_thought_maps_to_trace_update_and_counts() {
        let (bridge, mut rx) = bridge_with_channel();
        bridge.on_event(&sample_event(SystemEventType::ThoughtRecorded));
        bridge.on_event(&sample_event(SystemEventType::ThoughtRecorded));

        let first = rx.try_recv().unwrap();
        assert_eq!(first.event.as_deref(), Some("trace_update"));
        let step: serde_json::Value = serde_json::from_str(&first.data).unwrap();
        assert_eq!(step["label"], "payload text");
        assert_eq!(step["status"], "complete");

        assert_eq!(bridge.trace_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_event_type_mapping_matches_dashboard_union() {
        assert_eq!(
            dashboard_event_type(&SystemEventType::ThoughtRecorded),
            "thought"
        );
        assert_eq!(dashboard_event_type(&SystemEventType::AgentFailed), "error");
        assert_eq!(
            dashboard_event_type(&SystemEventType::AgentSpawned),
            "state_change"
        );
        assert_eq!(
            dashboard_event_type(&SystemEventType::MemoryStored),
            "message"
        );
    }

    #[test]
    fn test_agent_status_label_covers_dashboard_union() {
        assert_eq!(agent_status_label(&AgentState::Running), "running");
        assert_eq!(agent_status_label(&AgentState::Created), "starting");
        assert_eq!(agent_status_label(&AgentState::Stopped), "stopped");
        assert_eq!(
            agent_status_label(&AgentState::Degraded("slow".into())),
            "error"
        );
        assert_eq!(agent_status_label(&AgentState::Failed("x".into())), "error");
    }

    #[test]
    fn test_iso_timestamp_is_rfc3339() {
        let ts = iso_timestamp(1_700_000_000_000);
        assert!(ts.starts_with("2023-11-14T"), "unexpected timestamp: {ts}");
    }
}
