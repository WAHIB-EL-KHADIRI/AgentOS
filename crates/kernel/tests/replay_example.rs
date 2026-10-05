//! Keeps the recorded session in `examples/replay/` replaying with zero drift.
//!
//! Regenerate it after an intentional format change with:
//!
//! ```text
//! AGENTOS_BLESS_REPLAY_EXAMPLE=1 cargo test -p agentos-kernel --test replay_example -- --test-threads=1
//! ```

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agentos_kernel::{AgentOSSystem, AgentSpec, Persistence, RecordedSession, RuntimeConfig};
use agentos_llm::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, LLMProvider,
    LLMProviderError, LLMProviderResult, ProviderKind,
};

/// Agent id of the shipped example. The README tells visitors to run
/// `agentOS replay --session demo_agent`, so renaming it breaks that command.
const EXAMPLE_ID: &str = "demo_agent";
const EXAMPLE_NAME: &str = "demo-agent";
const EXAMPLE_PROMPT: &str =
    "You are a release assistant. Answer in one short paragraph and name the risk first.";
const EXAMPLE_INPUT: &str = "Is it safe to ship v0.2 if the migration test is flaky?";
/// Says what produced the recording. There is no real model behind it, and
/// the journal should not pretend otherwise.
const EXAMPLE_MODEL: &str = "scripted-offline";
const EXAMPLE_ANSWER: &str = "Risk first: a flaky migration test means you do not know \
     whether the migration is safe, only that it sometimes passes. Hold the release, \
     rerun the test in a loop to find the failing order, and ship once it passes \
     consistently or the flake is proven unrelated to the migration.";

fn example_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/replay")
}

fn temp_data_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "agentos_replay_example_{label}_{}",
        uuid::Uuid::new_v4()
    ))
}

/// Serves fixed responses in order. It stands in for a model so the example
/// can be produced by the real recording path with no network.
struct ScriptedProvider {
    responses: Mutex<VecDeque<ChatCompletionResponse>>,
}

#[async_trait::async_trait]
impl LLMProvider for ScriptedProvider {
    fn name(&self) -> &str {
        "scripted"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Custom
    }

    fn model(&self) -> &str {
        EXAMPLE_MODEL
    }

    async fn chat(
        &self,
        _request: ChatCompletionRequest,
    ) -> LLMProviderResult<ChatCompletionResponse> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| LLMProviderError::RequestFailed("scripted provider exhausted".into()))
    }

    async fn chat_stream(
        &self,
        _request: ChatCompletionRequest,
    ) -> LLMProviderResult<
        Box<dyn futures::Stream<Item = LLMProviderResult<ChatCompletionChunk>> + Send + Unpin>,
    > {
        Err(LLMProviderError::StreamError("not implemented".into()))
    }

    fn is_configured(&self) -> bool {
        true
    }
}

/// Record the example through the same path `agentOS run` uses, into
/// `data_dir`, and return what was journaled.
async fn record_example(data_dir: &std::path::Path) -> RecordedSession {
    let system = AgentOSSystem::with_config(RuntimeConfig {
        data_dir: data_dir.to_string_lossy().into_owned(),
        ..Default::default()
    });
    system
        .set_llm_provider(Arc::new(ScriptedProvider {
            responses: Mutex::new(VecDeque::from([ChatCompletionResponse {
                id: "scripted-1".into(),
                model: EXAMPLE_MODEL.into(),
                content: EXAMPLE_ANSWER.into(),
                tool_calls: Vec::new(),
                finish_reason: "stop".into(),
                usage: None,
            }])),
        }))
        .await;

    let mut spec = AgentSpec::new(EXAMPLE_ID, EXAMPLE_NAME);
    spec.prompt = EXAMPLE_PROMPT.into();
    system.spawn_agent(spec).await.expect("spawn example agent");
    system
        .run_agent_once(EXAMPLE_ID, EXAMPLE_INPUT)
        .await
        .expect("record example session");
    system.shutdown_all().await;

    Persistence::new(data_dir)
        .load_journal(EXAMPLE_ID)
        .await
        .expect("the run must have journaled a session")
}

async fn load_shipped_example() -> RecordedSession {
    Persistence::new(example_data_dir())
        .load_journal(EXAMPLE_ID)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "examples/replay must ship a journal for '{EXAMPLE_ID}' ({e}); \
                 regenerate with AGENTOS_BLESS_REPLAY_EXAMPLE=1"
            )
        })
}

/// Everything in a journal except the two fields that differ on every run
/// by construction: the wall-clock timestamp and the random checkpoint ids.
fn comparable(session: &RecordedSession) -> serde_json::Value {
    let mut value = serde_json::to_value(session).expect("journal serializes");
    let object = value.as_object_mut().expect("journal is an object");
    object.remove("recorded_at_ms");
    for exchange in object
        .get_mut("exchanges")
        .and_then(|e| e.as_array_mut())
        .expect("journal has exchanges")
    {
        exchange
            .as_object_mut()
            .expect("exchange is an object")
            .remove("checkpoint_id");
    }
    value
}

#[tokio::test]
async fn shipped_example_is_what_the_recorder_produces_today() {
    let dir = temp_data_dir("record");
    let fresh = record_example(&dir).await;

    if std::env::var_os("AGENTOS_BLESS_REPLAY_EXAMPLE").is_some() {
        Persistence::new(example_data_dir())
            .save_journal(&fresh)
            .await
            .expect("write examples/replay journal");
    }

    let shipped = load_shipped_example().await;
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        comparable(&shipped),
        comparable(&fresh),
        "examples/replay/journals/{EXAMPLE_ID}.json no longer matches what the \
         recorder writes. If the format change is intentional, regenerate it with \
         AGENTOS_BLESS_REPLAY_EXAMPLE=1 -- and note that every journal users \
         already have on disk changed with it."
    );
}

#[tokio::test]
async fn shipped_example_replays_with_zero_drift_in_a_fresh_system() {
    let shipped = load_shipped_example().await;
    assert!(
        !shipped.exchanges.is_empty(),
        "the example must contain at least one exchange to replay"
    );

    // A fresh system with no provider at all: nothing from the recording run
    // is in memory, and there is no network to fall back to. Replay output
    // goes to a temp dir so the test never writes into examples/.
    let dir = temp_data_dir("replay");
    let system = AgentOSSystem::with_config(RuntimeConfig {
        data_dir: dir.to_string_lossy().into_owned(),
        ..Default::default()
    });

    let (step, drifts) = system
        .replay_agent_session(&shipped)
        .await
        .expect("the shipped example must replay offline");
    system.shutdown_all().await;
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        drifts.is_empty(),
        "replaying the shipped example drifted from its recording: {drifts:#?}"
    );
    assert_eq!(step.provider, "replay");
    assert_eq!(step.exchanges.len(), shipped.exchanges.len());
    for (replayed, recorded) in step.exchanges.iter().zip(&shipped.exchanges) {
        assert_eq!(replayed.request_fingerprint, recorded.request_fingerprint);
    }
    assert_eq!(
        step.content,
        shipped.exchanges.last().unwrap().response.content,
        "replay must reproduce the recorded final answer"
    );
}
