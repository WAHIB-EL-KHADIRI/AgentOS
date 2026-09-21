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
    /// Stable fingerprint of the whole request — model, messages, tool set
    /// and sampling parameters — used to detect drift when replaying.
    /// Carries a `v<N>:` algorithm prefix; see [`request_fingerprint`].
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

/// Version tag for the fingerprint algorithm, emitted as a `v<N>:` prefix.
///
/// Journals recorded before versioning carry a bare hex digest. Bumping this
/// is what lets `compare_replay` say "this journal predates the current
/// algorithm" instead of mistaking an algorithm change for prompt drift.
const FINGERPRINT_VERSION: &str = "v2";

/// Length-prefix a field before hashing it.
///
/// The previous NUL-delimited scheme was ambiguous: message content is
/// arbitrary UTF-8 and may itself contain NUL, so two different message
/// sequences could serialise to the same byte stream. Length prefixes make
/// the encoding injective.
fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hash_optional(hasher: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(bytes) => {
            hasher.update([1u8]);
            hash_field(hasher, bytes);
        }
        None => hasher.update([0u8]),
    }
}

/// Stable fingerprint of a chat request, independent of hasher seeds and
/// toolchain versions (journals must stay comparable across builds).
///
/// Covers every field of the request that changes what the provider is asked
/// to do. The tool set matters as much as the prompt: replaying an agent that
/// has gained, lost or redefined a tool is not a faithful replay, even when
/// the messages are byte-identical. Sampling parameters matter for the same
/// reason — they change the space of responses the recording was drawn from.
pub fn request_fingerprint(request: &ChatCompletionRequest) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, FINGERPRINT_VERSION.as_bytes());
    hash_field(&mut hasher, request.model.as_bytes());

    hasher.update((request.messages.len() as u64).to_le_bytes());
    for message in &request.messages {
        hash_field(&mut hasher, format!("{:?}", message.role).as_bytes());
        hash_field(&mut hasher, message.content.as_bytes());
        hash_optional(&mut hasher, message.name.as_deref().map(str::as_bytes));
        hash_optional(
            &mut hasher,
            message.tool_call_id.as_deref().map(str::as_bytes),
        );
    }

    // `ToolRegistry::tools_for` already returns tools sorted by name, so
    // hashing in order is deterministic; it is deliberately order-sensitive,
    // because the order the provider is given is part of the request.
    hasher.update((request.tools.len() as u64).to_le_bytes());
    for tool in &request.tools {
        hash_field(&mut hasher, tool.name.as_bytes());
        hash_field(&mut hasher, tool.description.as_bytes());
        // serde_json keeps object keys in a BTreeMap unless `preserve_order`
        // is enabled, which this workspace does not enable, so the rendered
        // schema is canonical.
        hash_optional(
            &mut hasher,
            tool.parameters
                .as_ref()
                .map(|p| p.to_string())
                .as_deref()
                .map(str::as_bytes),
        );
    }

    // f32 via to_bits: exact and stable, unlike a formatted decimal.
    hash_optional(
        &mut hasher,
        request
            .temperature
            .map(|t| t.to_bits().to_le_bytes())
            .as_ref()
            .map(|b| &b[..]),
    );
    hash_optional(
        &mut hasher,
        request
            .top_p
            .map(|t| t.to_bits().to_le_bytes())
            .as_ref()
            .map(|b| &b[..]),
    );
    hash_optional(
        &mut hasher,
        request
            .max_tokens
            .map(|m| m.to_le_bytes())
            .as_ref()
            .map(|b| &b[..]),
    );
    hasher.update([request.stream as u8]);

    let digest = hasher.finalize();
    format!("{FINGERPRINT_VERSION}:{}", hex::encode(&digest[..8]))
}

/// The algorithm version a fingerprint was produced by. Journals written
/// before versioning have no prefix and report `None`.
fn fingerprint_version(fingerprint: &str) -> Option<&str> {
    let (version, rest) = fingerprint.split_once(':')?;
    // Guard against a bare digest that happens to contain a colon.
    version
        .strip_prefix('v')
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .filter(|_| !rest.is_empty())
        .map(|_| version)
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
    /// The recording predates the current fingerprint algorithm, so request
    /// equivalence could not be checked. Not evidence of drift — evidence
    /// that this particular check could not run.
    Unverifiable,
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
        if orig.request_fingerprint == replay.request_fingerprint {
            continue;
        }
        let recorded_version = fingerprint_version(&orig.request_fingerprint);
        let replay_version = fingerprint_version(&replay.request_fingerprint);
        if recorded_version != replay_version {
            // Different algorithms produce different digests for an
            // identical request, so this comparison carries no information.
            // Saying "prompt drift" here would be a false accusation.
            drifts.push(ReplayDrift {
                kind: DriftKind::Unverifiable,
                detail: format!(
                    "exchange {i}: recorded with fingerprint {}, replayed with {} \
                     — request equivalence not checked; re-record to verify",
                    recorded_version.unwrap_or("v1 (unversioned)"),
                    replay_version.unwrap_or("v1 (unversioned)")
                ),
            });
            continue;
        }
        drifts.push(ReplayDrift {
            kind: DriftKind::Request,
            detail: format!(
                "exchange {i}: request fingerprint changed ({} -> {})",
                orig.request_fingerprint, replay.request_fingerprint
            ),
        });
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

    /// The prompt is byte-identical in every case below; only the agent's
    /// capabilities or sampling settings differ. Each of these produced an
    /// identical fingerprint before the request was hashed in full, so a
    /// replay against a changed agent reported "no drift".
    #[test]
    fn fingerprint_covers_the_tool_set() {
        let base = request("m", "hello");

        let mut gained_a_tool = base.clone();
        gained_a_tool
            .tools
            .push(agentos_llm::ToolDefinition::new("shell", "Run a command"));

        let mut different_tool = base.clone();
        different_tool
            .tools
            .push(agentos_llm::ToolDefinition::new("http", "Run a command"));

        let mut redescribed = base.clone();
        redescribed.tools.push(agentos_llm::ToolDefinition::new(
            "shell",
            "Run a command as root",
        ));

        let mut reschemad = base.clone();
        reschemad.tools.push(
            agentos_llm::ToolDefinition::new("shell", "Run a command")
                .with_parameters(serde_json::json!({"type": "object"})),
        );

        let fingerprints: Vec<String> = [
            &base,
            &gained_a_tool,
            &different_tool,
            &redescribed,
            &reschemad,
        ]
        .iter()
        .map(|r| request_fingerprint(r))
        .collect();

        let mut unique = fingerprints.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            fingerprints.len(),
            "tool-set changes must be distinguishable: {fingerprints:?}"
        );
    }

    #[test]
    fn fingerprint_covers_sampling_parameters() {
        let base = request("m", "hello");

        let mut hotter = base.clone();
        hotter.temperature = Some(0.9);
        let mut cooler = base.clone();
        cooler.temperature = Some(0.1);
        let mut capped = base.clone();
        capped.max_tokens = Some(64);
        let mut nucleus = base.clone();
        nucleus.top_p = Some(0.5);
        let mut streaming = base.clone();
        streaming.stream = true;

        let fingerprints: Vec<String> = [&base, &hotter, &cooler, &capped, &nucleus, &streaming]
            .iter()
            .map(|r| request_fingerprint(r))
            .collect();

        let mut unique = fingerprints.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            fingerprints.len(),
            "sampling changes must be distinguishable: {fingerprints:?}"
        );
    }

    #[test]
    fn fingerprint_covers_tool_call_identity_on_messages() {
        let mut answers_one_call = ChatCompletionRequest::new("m", vec![Message::user("hi")]);
        answers_one_call.messages[0].tool_call_id = Some("call_1".into());
        let mut answers_another = ChatCompletionRequest::new("m", vec![Message::user("hi")]);
        answers_another.messages[0].tool_call_id = Some("call_2".into());

        assert_ne!(
            request_fingerprint(&answers_one_call),
            request_fingerprint(&answers_another),
            "a tool result answering a different call is a different request"
        );
    }

    /// Length prefixes make the encoding injective. The old scheme wrote
    /// `role \0 content \0` per message, so one message whose content
    /// embedded `\0User\0` serialised to exactly the same bytes as two
    /// messages: a user-controlled string could forge a message boundary.
    #[test]
    fn fingerprint_encoding_is_unambiguous() {
        let two_messages =
            ChatCompletionRequest::new("m", vec![Message::user("a"), Message::user("b")]);
        // Under the old encoding both render as "User\0a\0User\0b\0".
        let one_forged_message = ChatCompletionRequest::new("m", vec![Message::user("a\0User\0b")]);

        assert_ne!(
            request_fingerprint(&two_messages),
            request_fingerprint(&one_forged_message),
            "message boundaries must not be forgeable through content"
        );
    }

    #[test]
    fn old_journals_are_reported_unverifiable_not_drifted() {
        let original = RecordedSession {
            agent_id: "a".into(),
            agent_name: "a".into(),
            prompt: "p".into(),
            capabilities: Vec::new(),
            model: "m".into(),
            user_input: "u".into(),
            // A pre-versioning journal: bare digest, no `v2:` prefix.
            exchanges: vec![exchange("0123456789abcdef")],
            tool_invocations: Vec::new(),
            recorded_at_ms: 0,
        };
        let replayed = vec![exchange(&request_fingerprint(&request("m", "hello")))];

        let drifts = compare_replay(&original, &replayed, &[]);

        assert_eq!(drifts.len(), 1, "{drifts:?}");
        assert_eq!(
            drifts[0].kind,
            DriftKind::Unverifiable,
            "an algorithm change must not be reported as prompt drift: {}",
            drifts[0].detail
        );
    }

    #[test]
    fn same_version_mismatch_is_still_real_drift() {
        let original = RecordedSession {
            agent_id: "a".into(),
            agent_name: "a".into(),
            prompt: "p".into(),
            capabilities: Vec::new(),
            model: "m".into(),
            user_input: "u".into(),
            exchanges: vec![exchange(&request_fingerprint(&request("m", "hello")))],
            tool_invocations: Vec::new(),
            recorded_at_ms: 0,
        };
        let replayed = vec![exchange(&request_fingerprint(&request("m", "goodbye")))];

        let drifts = compare_replay(&original, &replayed, &[]);

        assert_eq!(drifts.len(), 1, "{drifts:?}");
        assert_eq!(drifts[0].kind, DriftKind::Request);
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

        // Versioned prefix plus the 16-hex-char digest. The prefix is what
        // lets a replay tell an algorithm change apart from prompt drift.
        let (version, digest) = a.split_once(':').expect("fingerprint carries a version");
        assert_eq!(version, FINGERPRINT_VERSION);
        assert_eq!(digest.len(), 16);
        assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
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
