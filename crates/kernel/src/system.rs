use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

use agentos_bus::InMemoryBus;
use agentos_llm::{
    anthropic::AnthropicProvider, ollama::OllamaProvider, openai::OpenAIProvider,
    ChatCompletionRequest, LLMProvider, Message, ToolCall, ToolDefinition,
};
use agentos_memory::{Embedder, HashingEmbedder, InMemoryStore, MemoryRecord, MemoryStore};
use agentos_registry::{Registry, ServiceDescriptor};
use agentos_trace::TraceRecorder;
use agentos_vault::{PermissionSet, Vault, VaultEncryption};
use tokio::sync::RwLock;

use crate::agent::AgentSpec;
use crate::error::AgentError;
use crate::events::{EventBus, SystemEventType};
use crate::handle::AgentHandle;
use crate::persistence::Persistence;
use crate::plugins::AgentHooks;
use crate::runtime_config::RuntimeConfig;
use crate::supervisor::Supervisor;
use crate::tools::{RuntimeTool, ToolRegistry};
use crate::AgentResult;

/// Upper bound on LLM tool rounds inside one execution step, so a model
/// that keeps requesting tools cannot loop forever.
const MAX_TOOL_ROUNDS: usize = 4;

pub type SystemResult<T> = Result<T, SystemError>;

#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("system not initialized: {0}")]
    NotInitialized(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct AgentExecutionStep {
    pub agent_id: String,
    pub provider: String,
    pub model: String,
    pub prompt_checkpoint_id: String,
    pub response_checkpoint_id: String,
    pub content: String,
    pub finish_reason: String,
    pub tool_call_count: usize,
    pub tool_invocations: Vec<ToolInvocationRecord>,
    pub rounds: usize,
}

/// One executed tool call inside an agent execution step.
#[derive(Debug, Clone)]
pub struct ToolInvocationRecord {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub success: bool,
    pub output: String,
    pub checkpoint_id: String,
}

pub struct AgentOSSystem {
    pub supervisor: Supervisor,
    pub bus: Arc<InMemoryBus>,
    pub trace_recorder: Arc<RwLock<TraceRecorder>>,
    pub memory_store: Arc<dyn MemoryStore>,
    pub vault: Arc<RwLock<Vault>>,
    pub registry: Arc<RwLock<Registry>>,
    pub embedder: Arc<dyn Embedder>,
    pub event_bus: Arc<EventBus>,
    pub agent_hooks: Arc<AgentHooks>,
    pub config: RuntimeConfig,
    pub tool_registry: Arc<ToolRegistry>,
    llm_provider: Arc<RwLock<Option<Arc<dyn LLMProvider>>>>,
    vault_encryption: Option<Arc<VaultEncryption>>,
    permission_set: Arc<RwLock<PermissionSet>>,
    agent_logs: Arc<RwLock<AgentLogStore>>,
}

impl fmt::Debug for AgentOSSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentOSSystem")
            .field("supervisor", &self.supervisor)
            .field("bus", &self.bus)
            .field("trace_recorder", &self.trace_recorder)
            .field("memory_store", &"Arc<dyn MemoryStore>")
            .field("vault", &self.vault)
            .field("registry", &self.registry)
            .field("embedder", &"Arc<dyn Embedder>")
            .field("llm_provider", &"Arc<RwLock<Option<Arc<dyn LLMProvider>>>>")
            .field("tool_registry", &self.tool_registry)
            .field(
                "vault_encryption",
                &self.vault_encryption.as_ref().map(|_| "configured"),
            )
            .field("permission_set", &self.permission_set)
            .field("agent_logs", &self.agent_logs)
            .finish()
    }
}

impl Default for AgentOSSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentOSSystem {
    pub fn new() -> Self {
        Self::with_config(RuntimeConfig::default())
    }

    pub fn with_config(config: RuntimeConfig) -> Self {
        let bus = Arc::new(InMemoryBus::new());
        let supervisor = Supervisor::new()
            .with_shared_bus(Arc::clone(&bus))
            .with_max_agents(config.max_agents);
        let event_bus = Arc::new(EventBus::new());
        let agent_hooks = Arc::new(AgentHooks::new());

        Self {
            supervisor,
            bus,
            trace_recorder: Arc::new(RwLock::new(TraceRecorder::new())),
            memory_store: Arc::new(InMemoryStore::new()),
            vault: Arc::new(RwLock::new(Vault::new())),
            registry: Arc::new(RwLock::new(Registry::new())),
            embedder: Arc::new(HashingEmbedder::new(128)),
            event_bus,
            agent_hooks,
            config,
            tool_registry: Arc::new(ToolRegistry::new()),
            llm_provider: Arc::new(RwLock::new(configured_llm_provider_from_env())),
            vault_encryption: vault_encryption_from_env(),
            permission_set: Arc::new(RwLock::new(PermissionSet::new())),
            agent_logs: Arc::new(RwLock::new(AgentLogStore::new())),
        }
    }

    /// Inject a vault encryption key directly (tests, embedders). Runtime
    /// callers normally configure it through `AGENTOS_VAULT_KEY`.
    pub fn with_vault_encryption(mut self, encryption: VaultEncryption) -> Self {
        self.vault_encryption = Some(Arc::new(encryption));
        self
    }

    pub fn has_vault_encryption(&self) -> bool {
        self.vault_encryption.is_some()
    }

    pub async fn spawn_agent(&self, spec: AgentSpec) -> AgentResult<AgentHandle> {
        let agent_id = spec.id.clone();
        let agent_name = spec.name.clone();
        let capabilities = spec.capabilities.clone();

        let handle = self.supervisor.spawn(spec).await?;

        // Register in registry
        {
            let mut reg = self.registry.write().await;
            let desc = ServiceDescriptor::new(&agent_id, "local").with_capabilities(capabilities);
            reg.register_or_update(desc);
        }

        // Record trace checkpoint for spawn
        {
            let mut trace = self.trace_recorder.write().await;
            trace.record_checkpoint(&agent_id, format!("Agent '{}' spawned", agent_name));
        }

        // Log the event
        self.log_event(
            &agent_id,
            "spawned",
            &format!("Agent '{}' started", agent_name),
        )
        .await;

        // Emit system event
        self.event_bus
            .emit(
                SystemEventType::AgentSpawned,
                Some(agent_id.to_string()),
                format!("Agent '{}' spawned", agent_name),
            )
            .await;

        // Notify plugins
        self.agent_hooks.on_spawned(self, &agent_id).await;

        Ok(handle)
    }

    pub async fn set_llm_provider(&self, provider: Arc<dyn LLMProvider>) {
        let mut configured = self.llm_provider.write().await;
        *configured = Some(provider);
    }

    pub async fn has_llm_provider(&self) -> bool {
        self.llm_provider.read().await.is_some()
    }

    /// Register a runtime tool for an agent so the execution loop can
    /// invoke it when the LLM requests it.
    pub async fn register_tool(&self, agent_id: &str, tool: Arc<dyn RuntimeTool>) {
        tracing::info!(agent_id = %agent_id, tool = %tool.name(), "runtime tool registered");
        self.tool_registry.register(agent_id, tool).await;
    }

    pub async fn run_agent_once(
        &self,
        agent_id: &str,
        user_input: impl Into<String>,
    ) -> AgentResult<AgentExecutionStep> {
        let handle = self
            .supervisor
            .get(agent_id)
            .await
            .ok_or_else(|| AgentError::NotFound(agent_id.to_string()))?;

        if !handle.is_running().await {
            return Err(AgentError::NotRunning(agent_id.to_string()));
        }

        let provider = self
            .llm_provider
            .read()
            .await
            .clone()
            .ok_or_else(|| AgentError::ConfigError("LLM provider is not configured".into()))?;

        let spec = handle.spec().clone();
        let system_prompt = if spec.prompt.trim().is_empty() {
            "You are a careful AgentOS agent.".to_string()
        } else {
            spec.prompt.clone()
        };
        let user_input = user_input.into();
        let provider_name = provider.name().to_string();
        let model = provider.model().to_string();

        let prompt_checkpoint_id = self
            .record_thought(agent_id, &format!("prompt: {user_input}"))
            .await;

        let tools = self.tool_registry.tools_for(agent_id).await;
        let tool_definitions: Vec<ToolDefinition> = tools
            .iter()
            .map(|tool| {
                let mut def = ToolDefinition::new(tool.name(), tool.description());
                if let Some(params) = tool.parameters() {
                    def = def.with_parameters(params);
                }
                def
            })
            .collect();

        let mut messages = vec![Message::system(system_prompt), Message::user(user_input)];
        let mut tool_invocations: Vec<ToolInvocationRecord> = Vec::new();
        let mut rounds = 0usize;

        loop {
            rounds += 1;
            let mut request = ChatCompletionRequest::new(model.clone(), messages.clone());
            request.tools = tool_definitions.clone();

            let response = provider
                .chat(request)
                .await
                .map_err(|e| AgentError::CommandFailed(format!("LLM chat failed: {e}")))?;

            // Final answer: no tool calls requested in this round.
            if response.tool_calls.is_empty() {
                let content = response.content.clone();
                let response_checkpoint_id = self
                    .record_thought(agent_id, &format!("assistant: {content}"))
                    .await;
                self.log_event(agent_id, "llm_response", &content).await;

                return Ok(AgentExecutionStep {
                    agent_id: agent_id.to_string(),
                    provider: provider_name,
                    model: if response.model.is_empty() {
                        model
                    } else {
                        response.model
                    },
                    prompt_checkpoint_id,
                    response_checkpoint_id,
                    content,
                    finish_reason: response.finish_reason,
                    tool_call_count: tool_invocations.len(),
                    tool_invocations,
                    rounds,
                });
            }

            // Tool round: execute every requested call and record each step.
            if !response.content.trim().is_empty() {
                self.record_thought(agent_id, &format!("assistant: {}", response.content))
                    .await;
            }

            let mut result_lines = Vec::new();
            for call in &response.tool_calls {
                let record = self.execute_tool_call(agent_id, call).await;
                result_lines.push(format!(
                    "{} -> {}",
                    record.name,
                    if record.success {
                        record.output.clone()
                    } else {
                        format!("error: {}", record.output)
                    }
                ));
                tool_invocations.push(record);
            }

            if rounds >= MAX_TOOL_ROUNDS {
                let content =
                    format!("tool round limit ({MAX_TOOL_ROUNDS}) reached before a final answer");
                let response_checkpoint_id = self
                    .record_thought(agent_id, &format!("assistant: {content}"))
                    .await;
                self.log_event(agent_id, "llm_response", &content).await;

                return Ok(AgentExecutionStep {
                    agent_id: agent_id.to_string(),
                    provider: provider_name,
                    model,
                    prompt_checkpoint_id,
                    response_checkpoint_id,
                    content,
                    finish_reason: "tool_rounds_exhausted".into(),
                    tool_call_count: tool_invocations.len(),
                    tool_invocations,
                    rounds,
                });
            }

            // Provider-agnostic continuation: tool results go back as a user
            // message so the loop works with OpenAI, Anthropic, and Ollama
            // alike. Native per-provider tool protocols (assistant.tool_calls
            // for OpenAI, tool_result blocks for Anthropic) belong in the llm
            // crate as a later improvement.
            let assistant_text = if response.content.trim().is_empty() {
                format!(
                    "(requested tools: {})",
                    response
                        .tool_calls
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                response.content.clone()
            };
            messages.push(Message::assistant(assistant_text));
            messages.push(Message::user(format!(
                "Tool results:\n{}",
                result_lines.join("\n")
            )));
        }
    }

    /// Execute one LLM tool call against the registry, recording checkpoints,
    /// logs, and system events. Tool failures are recorded, never fatal.
    async fn execute_tool_call(&self, agent_id: &str, call: &ToolCall) -> ToolInvocationRecord {
        let args_text = call.arguments.to_string();
        let call_summary = format!("{}({args_text})", call.name);

        {
            let mut trace = self.trace_recorder.write().await;
            trace.record_checkpoint(agent_id, format!("tool_call: {call_summary}"));
        }
        self.log_event(agent_id, "tool_call", &call_summary).await;
        self.event_bus
            .emit(
                SystemEventType::Custom("tool_call".into()),
                Some(agent_id.to_string()),
                call_summary.clone(),
            )
            .await;

        let outcome = match self.tool_registry.get(agent_id, &call.name).await {
            Some(tool) => tool.invoke(&call.arguments).await,
            None => Err(format!(
                "tool '{}' is not registered for this agent",
                call.name
            )),
        };
        let (success, output) = match outcome {
            Ok(output) => (true, output),
            Err(error) => (false, error),
        };

        let label = if success { "tool_result" } else { "tool_error" };
        let result_summary = format!("{} -> {output}", call.name);
        let checkpoint_id = {
            let mut trace = self.trace_recorder.write().await;
            trace.record_checkpoint(agent_id, format!("{label}: {result_summary}"))
        };
        self.log_event(agent_id, label, &result_summary).await;
        self.event_bus
            .emit(
                SystemEventType::Custom(label.into()),
                Some(agent_id.to_string()),
                result_summary,
            )
            .await;

        ToolInvocationRecord {
            call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            success,
            output,
            checkpoint_id,
        }
    }

    pub async fn record_thought(&self, agent_id: &str, content: &str) -> String {
        let mut trace = self.trace_recorder.write().await;
        let id = trace.record_checkpoint(agent_id, content);
        self.log_event(agent_id, "thought", content).await;

        self.event_bus
            .emit(
                SystemEventType::ThoughtRecorded,
                Some(agent_id.to_string()),
                content.to_string(),
            )
            .await;

        self.agent_hooks.on_thought(self, agent_id, content).await;

        id
    }

    pub async fn store_memory(&self, agent_id: &str, content: &str) -> AgentResult<String> {
        let mut record = MemoryRecord::new(agent_id, content);
        record.embedding = self.embedder.embed(content);
        let id = self
            .memory_store
            .insert(record)
            .map_err(|e| AgentError::Internal(e.to_string()))?;

        self.event_bus
            .emit(
                SystemEventType::MemoryStored,
                Some(agent_id.to_string()),
                content.to_string(),
            )
            .await;

        self.agent_hooks.on_memory(self, agent_id, content).await;

        Ok(id)
    }

    pub async fn search_memory(
        &self,
        agent_id: &str,
        query: &str,
        top_k: usize,
    ) -> AgentResult<Vec<MemoryRecord>> {
        let query_embedding = self.embedder.embed(query);
        self.memory_store
            .search(agent_id, &query_embedding, top_k)
            .map_err(|e| AgentError::Internal(e.to_string()))
    }

    pub async fn set_secret(&self, agent_id: &str, key: &str, value: &str) {
        {
            let mut vault = self.vault.write().await;
            vault.put(agent_id, key, value);
        }
        // Write-through: with a configured key, secrets survive restarts
        // encrypted. A persistence failure is logged, never fatal.
        if let Err(error) = self.persist_vault().await {
            tracing::warn!(%error, "vault write-through persistence failed");
        }
    }

    /// Persist the vault encrypted, when `AGENTOS_VAULT_KEY` is configured
    /// and a data directory is set. No-op otherwise (in-memory only).
    pub async fn persist_vault(&self) -> AgentResult<()> {
        let Some(encryption) = &self.vault_encryption else {
            return Ok(());
        };
        if self.config.data_dir.trim().is_empty() {
            return Ok(());
        }

        let persistence = Persistence::new(&self.config.data_dir);
        persistence.ensure_dirs().await?;
        let vault = self.vault.read().await;
        persistence.save_vault(&vault, encryption).await
    }

    /// Load previously persisted secrets into the vault. Returns the number
    /// of agents whose secrets were restored; `Ok(0)` when encryption is not
    /// configured or no vault file exists yet. Fails when a file exists but
    /// the key cannot decrypt it, so a wrong key is loud, not silent.
    pub async fn load_persisted_secrets(&self) -> AgentResult<usize> {
        let Some(encryption) = &self.vault_encryption else {
            return Ok(0);
        };
        if self.config.data_dir.trim().is_empty() {
            return Ok(0);
        }

        let persistence = Persistence::new(&self.config.data_dir);
        persistence.ensure_dirs().await?;
        let loaded = persistence.load_vault(encryption).await?;

        let mut vault = self.vault.write().await;
        let mut restored_agents = 0usize;
        for agent_id in loaded.agent_ids() {
            restored_agents += 1;
            for key in loaded.list_keys(&agent_id) {
                if let Some(value) = loaded.peek(&agent_id, &key) {
                    vault.put(&agent_id, &key, value);
                }
            }
        }
        Ok(restored_agents)
    }

    pub async fn get_secret(&self, agent_id: &str, key: &str) -> Option<String> {
        let mut vault = self.vault.write().await;
        let secret = vault.get(agent_id, key).ok()?;
        Some(secret.expose().to_string())
    }

    pub async fn register_service(&self, descriptor: ServiceDescriptor) {
        let mut reg = self.registry.write().await;
        reg.register_or_update(descriptor);
    }

    pub async fn discover_agents(&self, capability: &str) -> Vec<ServiceDescriptor> {
        let reg = self.registry.read().await;
        reg.discover_by_capability(capability)
            .into_iter()
            .cloned()
            .collect()
    }

    pub async fn log_event(&self, agent_id: &str, event_type: &str, message: &str) {
        let mut logs = self.agent_logs.write().await;
        logs.push(AgentLogEntry {
            agent_id: agent_id.to_string(),
            event_type: event_type.to_string(),
            message: message.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        });
    }

    pub async fn get_logs(&self, agent_id: &str, limit: usize) -> Vec<AgentLogEntry> {
        let logs = self.agent_logs.read().await;
        logs.get_for_agent(agent_id, limit)
    }

    pub async fn get_all_logs(&self) -> Vec<AgentLogEntry> {
        let logs = self.agent_logs.read().await;
        logs.all_entries()
    }

    pub async fn shutdown_all(&self) {
        let agents = self.supervisor.list().await;
        for handle in &agents {
            let _ = self.record_thought(&handle.id, "Agent shutting down").await;
        }
        self.supervisor.shutdown_all().await;
    }

    pub async fn grant_permission(&self, permission: agentos_vault::Permission) {
        let mut perms = self.permission_set.write().await;
        perms.grant(permission);
    }

    pub async fn check_permission(&self, permission: &agentos_vault::Permission) -> bool {
        let perms = self.permission_set.read().await;
        perms.contains(permission)
    }
}

/// Read the vault key from `AGENTOS_VAULT_KEY`. A malformed key logs an
/// error and disables persistence entirely (fail-safe: nothing is written
/// to disk rather than writing with an unintended key).
fn vault_encryption_from_env() -> Option<Arc<VaultEncryption>> {
    match VaultEncryption::from_env() {
        Ok(Some(encryption)) => Some(Arc::new(encryption)),
        Ok(None) => None,
        Err(error) => {
            tracing::error!(%error, "invalid AGENTOS_VAULT_KEY; encrypted vault persistence disabled");
            None
        }
    }
}

fn configured_llm_provider_from_env() -> Option<Arc<dyn LLMProvider>> {
    let provider = std::env::var("AGENTOS_LLM_PROVIDER")
        .unwrap_or_else(|_| "auto".to_string())
        .trim()
        .to_ascii_lowercase();

    match provider.as_str() {
        "openai" => OpenAIProvider::from_env().map(|p| Arc::new(p) as Arc<dyn LLMProvider>),
        "anthropic" => AnthropicProvider::from_env().map(|p| Arc::new(p) as Arc<dyn LLMProvider>),
        "ollama" => OllamaProvider::from_env().map(|p| Arc::new(p) as Arc<dyn LLMProvider>),
        "auto" | "" => OpenAIProvider::from_env()
            .map(|p| Arc::new(p) as Arc<dyn LLMProvider>)
            .or_else(|| AnthropicProvider::from_env().map(|p| Arc::new(p) as Arc<dyn LLMProvider>)),
        other => {
            tracing::warn!(provider = %other, "unknown AGENTOS_LLM_PROVIDER value");
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentLogEntry {
    pub agent_id: String,
    pub event_type: String,
    pub message: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Default)]
pub struct AgentLogStore {
    entries: VecDeque<AgentLogEntry>,
    max_entries: usize,
}

impl AgentLogStore {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries: 10_000,
        }
    }

    pub fn push(&mut self, entry: AgentLogEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn get_for_agent(&self, agent_id: &str, limit: usize) -> Vec<AgentLogEntry> {
        self.entries
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn all_entries(&self) -> Vec<AgentLogEntry> {
        self.entries.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentSpec;

    #[derive(Debug)]
    struct EchoProvider;

    #[async_trait::async_trait]
    impl LLMProvider for EchoProvider {
        fn name(&self) -> &str {
            "echo"
        }

        fn kind(&self) -> agentos_llm::ProviderKind {
            agentos_llm::ProviderKind::Custom
        }

        fn model(&self) -> &str {
            "echo-model"
        }

        async fn chat(
            &self,
            request: ChatCompletionRequest,
        ) -> agentos_llm::LLMProviderResult<agentos_llm::ChatCompletionResponse> {
            assert_eq!(request.model, "echo-model");
            assert_eq!(request.messages.len(), 2);
            Ok(agentos_llm::ChatCompletionResponse {
                id: "echo-1".into(),
                model: request.model,
                content: format!("echo: {}", request.messages[1].content),
                tool_calls: Vec::new(),
                finish_reason: "stop".into(),
                usage: None,
            })
        }

        async fn chat_stream(
            &self,
            _request: ChatCompletionRequest,
        ) -> agentos_llm::LLMProviderResult<
            Box<
                dyn futures::Stream<
                        Item = agentos_llm::LLMProviderResult<agentos_llm::ChatCompletionChunk>,
                    > + Send
                    + Unpin,
            >,
        > {
            Err(agentos_llm::LLMProviderError::StreamError(
                "not implemented".into(),
            ))
        }

        fn is_configured(&self) -> bool {
            true
        }
    }

    /// Returns queued responses in order; panics if exhausted.
    #[derive(Debug)]
    struct ScriptedProvider {
        responses: std::sync::Mutex<VecDeque<agentos_llm::ChatCompletionResponse>>,
    }

    impl ScriptedProvider {
        fn new(responses: Vec<agentos_llm::ChatCompletionResponse>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses.into_iter().collect()),
            }
        }
    }

    #[async_trait::async_trait]
    impl LLMProvider for ScriptedProvider {
        fn name(&self) -> &str {
            "scripted"
        }

        fn kind(&self) -> agentos_llm::ProviderKind {
            agentos_llm::ProviderKind::Custom
        }

        fn model(&self) -> &str {
            "scripted-model"
        }

        async fn chat(
            &self,
            _request: ChatCompletionRequest,
        ) -> agentos_llm::LLMProviderResult<agentos_llm::ChatCompletionResponse> {
            Ok(self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted provider exhausted"))
        }

        async fn chat_stream(
            &self,
            _request: ChatCompletionRequest,
        ) -> agentos_llm::LLMProviderResult<
            Box<
                dyn futures::Stream<
                        Item = agentos_llm::LLMProviderResult<agentos_llm::ChatCompletionChunk>,
                    > + Send
                    + Unpin,
            >,
        > {
            Err(agentos_llm::LLMProviderError::StreamError(
                "not implemented".into(),
            ))
        }

        fn is_configured(&self) -> bool {
            true
        }
    }

    struct UppercaseTool;

    #[async_trait::async_trait]
    impl RuntimeTool for UppercaseTool {
        fn name(&self) -> &str {
            "uppercase"
        }

        fn description(&self) -> &str {
            "Uppercase the given text"
        }

        async fn invoke(&self, arguments: &serde_json::Value) -> Result<String, String> {
            arguments
                .get("text")
                .and_then(|t| t.as_str())
                .map(|t| t.to_uppercase())
                .ok_or_else(|| "missing 'text' argument".to_string())
        }
    }

    fn tool_call_response(
        name: &str,
        args: serde_json::Value,
    ) -> agentos_llm::ChatCompletionResponse {
        agentos_llm::ChatCompletionResponse {
            id: "scripted-call".into(),
            model: "scripted-model".into(),
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: name.into(),
                arguments: args,
            }],
            finish_reason: "tool_calls".into(),
            usage: None,
        }
    }

    fn final_response(content: &str) -> agentos_llm::ChatCompletionResponse {
        agentos_llm::ChatCompletionResponse {
            id: "scripted-final".into(),
            model: "scripted-model".into(),
            content: content.into(),
            tool_calls: Vec::new(),
            finish_reason: "stop".into(),
            usage: None,
        }
    }

    #[tokio::test]
    async fn test_run_agent_once_executes_registered_tool() {
        let system = AgentOSSystem::new();
        system
            .set_llm_provider(Arc::new(ScriptedProvider::new(vec![
                tool_call_response("uppercase", serde_json::json!({"text": "hi"})),
                final_response("done: HI"),
            ])))
            .await;

        system
            .register_tool("tool-test", Arc::new(UppercaseTool))
            .await;
        system
            .spawn_agent(AgentSpec::new("tool-test", "Tool Test"))
            .await
            .unwrap();

        let step = system
            .run_agent_once("tool-test", "please uppercase")
            .await
            .unwrap();

        assert_eq!(step.rounds, 2);
        assert_eq!(step.tool_call_count, 1);
        assert_eq!(step.tool_invocations.len(), 1);
        let invocation = &step.tool_invocations[0];
        assert!(invocation.success);
        assert_eq!(invocation.name, "uppercase");
        assert_eq!(invocation.output, "HI");
        assert!(!invocation.checkpoint_id.is_empty());
        assert_eq!(step.content, "done: HI");
        assert_eq!(step.finish_reason, "stop");

        let logs = system.get_logs("tool-test", 50).await;
        assert!(logs.iter().any(|e| e.event_type == "tool_call"));
        assert!(logs.iter().any(|e| e.event_type == "tool_result"));

        let events = system.event_bus.read_for_agent("tool-test").await;
        assert!(events
            .iter()
            .any(|e| e.event_type == SystemEventType::Custom("tool_call".into())));
        assert!(events
            .iter()
            .any(|e| e.event_type == SystemEventType::Custom("tool_result".into())));

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_run_agent_once_unknown_tool_records_error() {
        let system = AgentOSSystem::new();
        system
            .set_llm_provider(Arc::new(ScriptedProvider::new(vec![
                tool_call_response("missing_tool", serde_json::json!({})),
                final_response("recovered"),
            ])))
            .await;
        system
            .spawn_agent(AgentSpec::new("missing-tool-test", "Missing Tool Test"))
            .await
            .unwrap();

        let step = system
            .run_agent_once("missing-tool-test", "go")
            .await
            .unwrap();

        assert_eq!(step.tool_invocations.len(), 1);
        assert!(!step.tool_invocations[0].success);
        assert!(step.tool_invocations[0].output.contains("not registered"));
        assert_eq!(step.content, "recovered");

        let logs = system.get_logs("missing-tool-test", 50).await;
        assert!(logs.iter().any(|e| e.event_type == "tool_error"));

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_run_agent_once_tool_round_cap() {
        let responses = (0..MAX_TOOL_ROUNDS)
            .map(|_| tool_call_response("uppercase", serde_json::json!({"text": "loop"})))
            .collect::<Vec<_>>();

        let system = AgentOSSystem::new();
        system
            .set_llm_provider(Arc::new(ScriptedProvider::new(responses)))
            .await;
        system
            .register_tool("loop-test", Arc::new(UppercaseTool))
            .await;
        system
            .spawn_agent(AgentSpec::new("loop-test", "Loop Test"))
            .await
            .unwrap();

        let step = system.run_agent_once("loop-test", "go").await.unwrap();

        assert_eq!(step.finish_reason, "tool_rounds_exhausted");
        assert_eq!(step.rounds, MAX_TOOL_ROUNDS);
        assert_eq!(step.tool_call_count, MAX_TOOL_ROUNDS);
        assert!(step.tool_invocations.iter().all(|t| t.success));

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_encrypted_secret_persistence_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("agentos_test_sys_vault_{}", uuid::Uuid::new_v4()));
        let config = RuntimeConfig {
            data_dir: dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        let encryption = agentos_vault::VaultEncryption::new();
        let key_hex = encryption.export_key();

        // First system: store a secret; write-through persists it encrypted.
        let system = AgentOSSystem::with_config(config.clone())
            .with_vault_encryption(agentos_vault::VaultEncryption::from_hex(&key_hex).unwrap());
        assert!(system.has_vault_encryption());
        system
            .set_secret("agent-1", "API_KEY", "sk-persisted")
            .await;

        let raw = std::fs::read(dir.join("vault").join("secrets.enc")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("sk-persisted"));

        // Second system with the same key: secrets are restored.
        let restored = AgentOSSystem::with_config(config.clone())
            .with_vault_encryption(agentos_vault::VaultEncryption::from_hex(&key_hex).unwrap());
        let agents = restored.load_persisted_secrets().await.unwrap();
        assert_eq!(agents, 1);
        assert_eq!(
            restored.get_secret("agent-1", "API_KEY").await,
            Some("sk-persisted".into())
        );

        // A system with a different key must fail loudly, not silently.
        let wrong = AgentOSSystem::with_config(config)
            .with_vault_encryption(agentos_vault::VaultEncryption::new());
        assert!(wrong.load_persisted_secrets().await.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_secrets_stay_in_memory_without_encryption_key() {
        let dir =
            std::env::temp_dir().join(format!("agentos_test_sys_novault_{}", uuid::Uuid::new_v4()));
        let config = RuntimeConfig {
            data_dir: dir.to_string_lossy().to_string(),
            ..Default::default()
        };

        let system = AgentOSSystem::with_config(config);
        system.set_secret("agent-1", "API_KEY", "sk-memory").await;

        // No key configured: nothing may be written to disk.
        assert!(!dir.join("vault").join("secrets.enc").exists());
        assert_eq!(system.load_persisted_secrets().await.unwrap(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_system_spawn_and_trace() {
        let system = AgentOSSystem::new();
        let spec = AgentSpec::new("sys-test-1", "System Test");
        let handle = system.spawn_agent(spec).await.unwrap();
        assert!(handle.is_running().await);

        let thought_id = system.record_thought("sys-test-1", "test thought").await;
        assert!(!thought_id.is_empty());

        system.shutdown_all().await;
        assert!(!handle.is_running().await);
    }

    #[tokio::test]
    async fn test_system_run_agent_once_calls_llm_and_records_trace() {
        let system = AgentOSSystem::new();
        system.set_llm_provider(Arc::new(EchoProvider)).await;

        let mut spec = AgentSpec::new("llm-test", "LLM Test");
        spec.prompt = "You are under test.".into();
        let handle = system.spawn_agent(spec).await.unwrap();
        assert!(handle.is_running().await);

        let step = system.run_agent_once("llm-test", "hello").await.unwrap();
        assert_eq!(step.provider, "echo");
        assert_eq!(step.model, "echo-model");
        assert_eq!(step.content, "echo: hello");
        assert_eq!(step.finish_reason, "stop");
        assert_eq!(step.tool_call_count, 0);
        assert!(!step.prompt_checkpoint_id.is_empty());
        assert!(!step.response_checkpoint_id.is_empty());

        let logs = system.get_logs("llm-test", 10).await;
        assert!(logs.iter().any(|entry| entry.event_type == "thought"));
        assert!(logs.iter().any(|entry| entry.event_type == "llm_response"));

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_system_memory_integration() {
        let system = AgentOSSystem::new();
        let id = system
            .store_memory("agent-1", "Important memory")
            .await
            .unwrap();
        assert!(!id.is_empty());

        let results = system.search_memory("agent-1", "memory", 5).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_system_vault_integration() {
        let system = AgentOSSystem::new();
        system.set_secret("agent-1", "API_KEY", "sk-123").await;
        let value = system.get_secret("agent-1", "API_KEY").await;
        assert_eq!(value, Some("sk-123".into()));
    }

    #[tokio::test]
    async fn test_system_registry_integration() {
        let system = AgentOSSystem::new();
        let desc = ServiceDescriptor::new("search-agent", "localhost:9001")
            .with_capabilities(vec!["search".into()]);
        system.register_service(desc).await;

        let results = system.discover_agents("search").await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_system_logging() {
        let system = AgentOSSystem::new();
        system.log_event("agent-1", "test", "hello world").await;
        let logs = system.get_logs("agent-1", 10).await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "hello world");
    }

    #[tokio::test]
    async fn test_system_with_config() {
        let cfg = RuntimeConfig {
            http_port: 9090,
            max_agents: 5,
            ..Default::default()
        };
        let system = AgentOSSystem::with_config(cfg);
        assert_eq!(system.config.http_port, 9090);
        assert_eq!(system.config.max_agents, 5);
    }

    #[tokio::test]
    async fn test_system_spawn_records_registry() {
        let system = AgentOSSystem::new();
        let mut spec = AgentSpec::new("reg-test", "Registry Test");
        spec.capabilities = vec!["search".into(), "memory".into()];
        let _handle = system.spawn_agent(spec).await.unwrap();

        let agents = system.discover_agents("search").await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "reg-test");
    }

    #[tokio::test]
    async fn test_system_spawn_failure_has_no_success_side_effects() {
        let cfg = RuntimeConfig {
            max_agents: 0,
            ..Default::default()
        };
        let system = AgentOSSystem::with_config(cfg);
        let mut spec = AgentSpec::new("spawn-fail", "Spawn Fail");
        spec.capabilities = vec!["search".into()];

        assert!(system.spawn_agent(spec).await.is_err());
        assert!(system
            .event_bus
            .read_for_agent("spawn-fail")
            .await
            .is_empty());
        assert!(system.get_logs("spawn-fail", 10).await.is_empty());
        assert!(system.discover_agents("search").await.is_empty());
    }

    #[tokio::test]
    async fn test_system_duplicate_spawn_emits_one_spawn_event() {
        let system = AgentOSSystem::new();
        system
            .spawn_agent(AgentSpec::new("dup-event", "Duplicate Event"))
            .await
            .unwrap();

        assert!(system
            .spawn_agent(AgentSpec::new("dup-event", "Duplicate Event"))
            .await
            .is_err());

        let events = system.event_bus.read_for_agent("dup-event").await;
        let spawned_events = events
            .iter()
            .filter(|event| event.event_type == SystemEventType::AgentSpawned)
            .count();
        assert_eq!(spawned_events, 1);

        let logs = system.get_logs("dup-event", 10).await;
        let spawned_logs = logs
            .iter()
            .filter(|entry| entry.event_type == "spawned")
            .count();
        assert_eq!(spawned_logs, 1);

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_system_and_supervisor_share_same_bus() {
        use agentos_bus::{AgentBusTrait, AgentEnvelope};
        let system = AgentOSSystem::new();

        let env = AgentEnvelope::new("src", "target-agent", "test", b"hello".to_vec());
        system.bus.publish(env).await.unwrap();

        let drained = system.supervisor.drain_bus_for("target-agent").await;
        assert!(
            drained.is_some(),
            "supervisor should see messages published on system.bus"
        );
        assert_eq!(drained.unwrap().len(), 1);
    }
}
