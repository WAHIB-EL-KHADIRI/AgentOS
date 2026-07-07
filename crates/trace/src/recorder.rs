use serde::{Deserialize, Serialize};

const MAX_CONTENT_LEN: usize = 1_000_000;
const MAX_METADATA_KEY_LEN: usize = 256;
const MAX_METADATA_VAL_LEN: usize = 10_000;
const MAX_METADATA_ENTRIES: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedThought {
    pub checkpoint_id: String,
    pub agent_id: String,
    pub content: String,
    pub timestamp_ms: u64,
    pub parent_checkpoint_id: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl RecordedThought {
    pub fn new(agent_id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut content: String = content.into();
        if content.len() > MAX_CONTENT_LEN {
            content.truncate(MAX_CONTENT_LEN);
        }
        Self {
            checkpoint_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            content,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            parent_checkpoint_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent_checkpoint_id = Some(parent.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if self.metadata.len() >= MAX_METADATA_ENTRIES {
            return self;
        }
        let mut key: String = key.into();
        let mut value: String = value.into();
        if key.len() > MAX_METADATA_KEY_LEN {
            key.truncate(MAX_METADATA_KEY_LEN);
        }
        if value.len() > MAX_METADATA_VAL_LEN {
            value.truncate(MAX_METADATA_VAL_LEN);
        }
        self.metadata.insert(key, value);
        self
    }
}

#[derive(Debug, Default)]
pub struct TraceRecorder {
    thoughts: Vec<RecordedThought>,
}

impl TraceRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, thought: RecordedThought) -> String {
        let id = thought.checkpoint_id.clone();
        self.thoughts.push(thought);
        id
    }

    pub fn record_checkpoint(
        &mut self,
        agent_id: impl Into<String>,
        content: impl Into<String>,
    ) -> String {
        let thought = RecordedThought::new(agent_id, content);
        self.record(thought)
    }

    pub fn fork_from(
        &mut self,
        parent_checkpoint_id: &str,
        agent_id: impl Into<String>,
        content: impl Into<String>,
    ) -> String {
        let thought = RecordedThought::new(agent_id, content).with_parent(parent_checkpoint_id);
        self.record(thought)
    }

    pub fn thoughts(&self) -> &[RecordedThought] {
        &self.thoughts
    }

    pub fn thoughts_for_agent(&self, agent_id: &str) -> Vec<&RecordedThought> {
        self.thoughts
            .iter()
            .filter(|t| t.agent_id == agent_id)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.thoughts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.thoughts.is_empty()
    }

    pub fn get_by_checkpoint(&self, checkpoint_id: &str) -> Option<&RecordedThought> {
        self.thoughts
            .iter()
            .find(|t| t.checkpoint_id == checkpoint_id)
    }

    pub fn branches(&self) -> Vec<Vec<&RecordedThought>> {
        let mut branches: Vec<Vec<&RecordedThought>> = Vec::new();
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();

        for thought in &self.thoughts {
            if !visited.contains(&thought.checkpoint_id) {
                let mut branch = Vec::new();
                self.collect_branch(thought, &mut branch, &mut visited);
                if !branch.is_empty() {
                    branches.push(branch);
                }
            }
        }

        branches
    }

    fn collect_branch<'a>(
        &'a self,
        current: &'a RecordedThought,
        branch: &mut Vec<&'a RecordedThought>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if !visited.insert(current.checkpoint_id.clone()) {
            return;
        }
        branch.push(current);
        for next in &self.thoughts {
            if next.parent_checkpoint_id.as_deref() == Some(&current.checkpoint_id) {
                self.collect_branch(next, branch, visited);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_thought() {
        let mut recorder = TraceRecorder::new();
        let thought = RecordedThought::new("agent-1", "test thought");
        let id = recorder.record(thought);
        assert_eq!(recorder.thoughts().len(), 1);
        assert!(!id.is_empty());
    }

    #[test]
    fn test_record_checkpoint() {
        let mut recorder = TraceRecorder::new();
        let id = recorder.record_checkpoint("agent-1", "checkpoint content");
        assert!(!id.is_empty());
        assert_eq!(recorder.thoughts().len(), 1);
    }

    #[test]
    fn test_fork_from_checkpoint() {
        let mut recorder = TraceRecorder::new();
        let parent_id = recorder.record_checkpoint("agent-1", "original");
        let fork_id = recorder.fork_from(&parent_id, "agent-1", "forked");
        let fork_thought = recorder.get_by_checkpoint(&fork_id).unwrap();
        assert_eq!(
            fork_thought.parent_checkpoint_id.as_deref(),
            Some(parent_id.as_str())
        );
    }

    #[test]
    fn test_get_by_checkpoint() {
        let mut recorder = TraceRecorder::new();
        let id = recorder.record_checkpoint("agent-1", "test");
        assert!(recorder.get_by_checkpoint(&id).is_some());
        assert!(recorder.get_by_checkpoint("nonexistent").is_none());
    }

    #[test]
    fn test_thoughts_for_agent() {
        let mut recorder = TraceRecorder::new();
        recorder.record_checkpoint("agent-1", "a");
        recorder.record_checkpoint("agent-1", "b");
        recorder.record_checkpoint("agent-2", "c");
        assert_eq!(recorder.thoughts_for_agent("agent-1").len(), 2);
        assert_eq!(recorder.thoughts_for_agent("agent-2").len(), 1);
    }

    #[test]
    fn test_thought_metadata() {
        let thought = RecordedThought::new("agent-1", "test")
            .with_metadata("tool", "search")
            .with_metadata("model", "gpt-4");
        assert_eq!(thought.metadata.get("tool").unwrap(), "search");
        assert_eq!(thought.metadata.get("model").unwrap(), "gpt-4");
    }

    #[test]
    fn test_branch_detection() {
        let mut recorder = TraceRecorder::new();
        let root = recorder.record_checkpoint("agent-1", "root");
        let _b1 = recorder.record_checkpoint("agent-1", "branch1.1");
        let _b1_2 = recorder.record_checkpoint("agent-1", "branch1.2");
        let _b2 = recorder.fork_from(&root, "agent-1", "branch2.1");

        let branches = recorder.branches();
        assert!(branches.len() >= 2);
    }
}
