use agentos_kernel::{AgentError, AgentReadiness, AgentSpec, Supervisor};
use agentos_sdk::{AgentBuilder, AgentConfig};

struct ReadyNow;

#[async_trait::async_trait]
impl AgentReadiness for ReadyNow {
    async fn wait_until_ready(&self, _spec: &AgentSpec) -> Result<(), String> {
        Ok(())
    }
}

struct FailReady(&'static str);

#[async_trait::async_trait]
impl AgentReadiness for FailReady {
    async fn wait_until_ready(&self, _spec: &AgentSpec) -> Result<(), String> {
        Err(self.0.to_string())
    }
}

#[test]
fn legacy_agent_config_literal_remains_source_compatible() {
    let config = AgentConfig {
        name: "legacy".into(),
        prompt: "A legacy agent".into(),
        capabilities: vec!["memory".into()],
        max_restarts: 5,
        heartbeat_timeout_secs: 30,
    };

    assert_eq!(config.name, "legacy");
    assert_eq!(config.heartbeat_timeout_secs, 30);
}

#[tokio::test]
async fn builder_readiness_timeout_reaches_supervised_agent_spec() {
    let supervisor = Supervisor::new();
    let handle = AgentBuilder::new("sdk-ready")
        .readiness_timeout_secs(17)
        .spawn_on_supervisor_with_readiness(&supervisor, ReadyNow)
        .await
        .expect("readiness success should spawn");

    assert_eq!(handle.spec().readiness_timeout_secs, 17);
    assert_eq!(
        supervisor
            .get("sdk-ready")
            .await
            .unwrap()
            .spec()
            .readiness_timeout_secs,
        17
    );

    supervisor.stop("sdk-ready").await.unwrap();
}

#[tokio::test]
async fn supervised_readiness_failure_uses_public_sdk_api() {
    let supervisor = Supervisor::new();
    let error = AgentBuilder::new("sdk-fail")
        .spawn_on_supervisor_with_readiness(&supervisor, FailReady("dependency unavailable"))
        .await
        .expect_err("readiness failure should reject the spawn");

    assert!(matches!(
        error,
        agentos_sdk::SdkError::Kernel(AgentError::CommandFailed(reason))
            if reason == "dependency unavailable"
    ));
    assert!(supervisor.get("sdk-fail").await.is_none());
}
