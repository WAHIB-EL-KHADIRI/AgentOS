//! Execution journals: structured recordings of agent execution sessions
//! (LLM exchanges + tool invocations), sufficient to re-execute a session
//! deterministically at the LLM boundary via `ReplayProvider`, and to
//! detect drift between a recording and a replay.

use agentos_llm::{ChatCompletionRequest, RecordedResponse};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::system::ToolInvocationRecord;

/// One LLM request/response exchange inside an execution session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedExchange {
    /// Stable fingerprint of the request (model + message roles/contents),
    /// used to detect prompt drift when replaying.
    pub request_fingerprint: String,
    /// Trace checkpoint id anchoring this exchange in the recorded trace.
    /// For the final round this is the assistant response checkpoint; for
    /// tool rounds it is the first tool result checkpoint of the round.
    #[serde(default)]
    pub checkpoint_id: String,
    pub response: RecordedResponse,
}

/// A recorded tool invocation, kept for drift detection on replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedToolInvocation {
    pub name: String,
    pub arguments: serde_json::Value,
    pub success: bool,
    pub output: String,
}

impl RecordedToolInvocation {
    pub fn from_record(record: &ToolInvocationRecord) -> Self {
        Self {
            name: record.name.clone(),
            arguments: record.arguments.clone(),
            success: record.success,
            output: record.output.clone(),
        }
    }
}

/// A full recorded execution session for one agent step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedSession {
    pub agent_id: String,
    pub agent_name: String,
    pub prompt: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The request model used during recording; replay impersonates it so
    /// request fingerprints stay comparable.
    #[serde(default)]
    pub model: String,
    pub user_input: String,
    pub exchanges: Vec<RecordedExchange>,
    #[serde(default)]
    pub tool_invocations: Vec<RecordedToolInvocation>,
    pub recorded_at_ms: u64,
}

/// Stable fingerprint of a chat request, independent of hasher seeds and
/// toolchain versions (journals must stay comparable across builds).
pub fn request_fingerprint(request: &ChatCompletionRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request.model.as_bytes());
    for message in &request.messages {
        hasher.update(format!("{:?}", message.role).as_bytes());
        hasher.update([0]);
        hasher.update(message.content.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

/// A detected difference between a recording and its replay.
#[derive(Debug, Clone)]
pub struct ReplayDrift {
    pub kind: DriftKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftKind {
    /// The replayed request differed from the recorded one (prompt drift).
    Request,
    /// A tool produced a different result than during recording.
    Tool,
    /// Recording and replay have different shapes (counts).
    Shape,
}

/// Compare an original session with the step produced by replaying it.
/// An empty result means the replay was faithful.
pub fn compare_replay(
    original: &RecordedSession,
    replayed_exchanges: &[RecordedExchange],
    replayed_tools: &[ToolInvocationRecord],
) -> Vec<ReplayDrift> {
    let mut drifts = Vec::new();

    if original.exchanges.len() != replayed_exchanges.len() {
        drifts.push(ReplayDrift {
            kind: DriftKind::Shape,
            detail: format!(
                "exchange count changed: recorded {}, replayed {}",
                original.exchanges.len(),
                replayed_exchanges.len()
            ),
        });
    }
    for (i, (orig, replay)) in original
        .exchanges
        .iter()
        .zip(replayed_exchanges.iter())
        .enumerate()
    {
        if orig.request_fingerprint != replay.request_fingerprint {
            drifts.push(ReplayDrift {
                kind: DriftKind::Request,
                detail: format!(
                    "exchange {i}: request fingerprint changed ({} -> {})",
                    orig.request_fingerprint, replay.request_fingerprint
                ),
            });
        }
    }

    if original.tool_invocations.len() != replayed_tools.len() {
        drifts.push(ReplayDrift {
            kind: DriftKind::Shape,
            detail: format!(
                "tool invocation count changed: recorded {}, replayed {}",
                original.tool_invocations.len(),
                replayed_tools.len()
            ),
        });
    }
    for (i, (orig, replay)) in original
        .tool_invocations
        .iter()
        .zip(replayed_tools.iter())
        .enumerate()
    {
        if orig.name != replay.name {
            drifts.push(ReplayDrift {
                kind: DriftKind::Tool,
                detail: format!(
                    "tool call {i}: name changed ({} -> {})",
                    orig.name, replay.name
                ),
            });
            continue;
        }
        if orig.arguments != replay.arguments {
            drifts.push(ReplayDrift {
                kind: DriftKind::Tool,
                detail: format!("tool call {i} ({}): arguments changed", orig.name),
            });
        }
        if orig.success != replay.success || orig.output != replay.output {
            drifts.push(ReplayDrift {
                kind: DriftKind::Tool,
                detail: format!(
                    "tool call {i} ({}): result changed (recorded {} '{}', replayed {} '{}')",
                    orig.name,
                    if orig.success { "ok" } else { "err" },
                    orig.output,
                    if replay.success { "ok" } else { "err" },
                    replay.output
                ),
            });
        }
    }

    drifts
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentos_llm::Message;

    fn request(model: &str, content: &str) -> ChatCompletionRequest {
        ChatCompletionRequest::new(model, vec![Message::user(content)])
    }

    fn exchange(fingerprint: &str) -> RecordedExchange {
        RecordedExchange {
            request_fingerprint: fingerprint.into(),
            checkpoint_id: String::new(),
            response: RecordedResponse {
                model: "m".into(),
                content: "c".into(),
                tool_calls: Vec::new(),
                finish_reason: "stop".into(),
            },
        }
    }

    fn tool_record(name: &str, output: &str, success: bool) -> ToolInvocationRecord {
        ToolInvocationRecord {
            call_id: "call".into(),
            name: name.into(),
            arguments: serde_json::json!({}),
            success,
            output: output.into(),
            checkpoint_id: String::new(),
        }
    }

    fn session(
        exchanges: Vec<RecordedExchange>,
        tools: Vec<RecordedToolInvocation>,
    ) -> RecordedSession {
        RecordedSession {
            agent_id: "a".into(),
            agent_name: "A".into(),
            prompt: "p".into(),
            capabilities: Vec::new(),
            model: "m".into(),
            user_input: "u".into(),
            exchanges,
            tool_invocations: tools,
            recorded_at_ms: 0,
        }
    }

    #[test]
    fn test_fingerprint_stable_and_sensitive() {
        let a = request_fingerprint(&request("m", "hello"));
        let b = request_fingerprint(&request("m", "hello"));
        assert_eq!(a, b);

        assert_ne!(a, request_fingerprint(&request("m", "other")));
        assert_ne!(a, request_fingerprint(&request("m2", "hello")));
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn test_faithful_replay_has_no_drift() {
        let original = session(
            vec![exchange("f1"), exchange("f2")],
            vec![RecordedToolInvocation::from_record(&tool_record(
                "lint", "ok", true,
            ))],
        );
        let drifts = compare_replay(
            &original,
            &[exchange("f1"), exchange("f2")],
            &[tool_record("lint", "ok", true)],
        );
        assert!(drifts.is_empty(), "unexpected drift: {drifts:?}");
    }

    #[test]
    fn test_request_and_tool_drift_detected() {
        let original = session(
            vec![exchange("f1")],
            vec![RecordedToolInvocation::from_record(&tool_record(
                "lint", "ok", true,
            ))],
        );

        let drifts = compare_replay(
            &original,
            &[exchange("CHANGED")],
            &[tool_record("lint", "different output", true)],
        );
        assert_eq!(drifts.len(), 2);
        assert!(drifts.iter().any(|d| d.kind == DriftKind::Request));
        assert!(drifts.iter().any(|d| d.kind == DriftKind::Tool));
    }

    #[test]
    fn test_shape_drift_detected() {
        let original = session(vec![exchange("f1"), exchange("f2")], Vec::new());
        let drifts = compare_replay(&original, &[exchange("f1")], &[]);
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].kind, DriftKind::Shape);
    }

    #[test]
    fn test_session_serde_roundtrip() {
        let original = session(vec![exchange("f1")], Vec::new());
        let json = serde_json::to_string(&original).unwrap();
        let parsed: RecordedSession = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, "a");
        assert_eq!(parsed.exchanges.len(), 1);
        assert_eq!(parsed.exchanges[0].request_fingerprint, "f1");
    }
}
