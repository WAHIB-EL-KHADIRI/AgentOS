use crate::recorder::RecordedThought;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayCursor {
    pub index: usize,
}

impl ReplayCursor {
    pub fn new(index: usize) -> Self {
        Self { index }
    }
}

#[derive(Debug, Clone)]
pub struct TraceReplayer {
    thoughts: Vec<RecordedThought>,
    cursor: ReplayCursor,
}

impl TraceReplayer {
    pub fn new(thoughts: Vec<RecordedThought>) -> Self {
        Self {
            thoughts,
            cursor: ReplayCursor { index: 0 },
        }
    }

    pub fn cursor(&self) -> ReplayCursor {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: ReplayCursor) {
        self.cursor = cursor;
    }

    pub fn seek(&mut self, checkpoint_id: &str) -> Option<&RecordedThought> {
        let index = self
            .thoughts
            .iter()
            .position(|thought| thought.checkpoint_id == checkpoint_id)?;
        self.cursor.index = index;
        self.thoughts.get(index)
    }

    pub fn current(&self) -> Option<&RecordedThought> {
        self.thoughts.get(self.cursor.index)
    }

    pub fn step_forward(&mut self) -> Option<&RecordedThought> {
        if self.cursor.index + 1 < self.thoughts.len() {
            self.cursor.index += 1;
        }
        self.current()
    }

    pub fn step_backward(&mut self) -> Option<&RecordedThought> {
        if self.cursor.index > 0 {
            self.cursor.index -= 1;
        }
        self.current()
    }

    pub fn reset(&mut self) {
        self.cursor.index = 0;
    }

    pub fn len(&self) -> usize {
        self.thoughts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.thoughts.is_empty()
    }

    pub fn thoughts(&self) -> &[RecordedThought] {
        &self.thoughts
    }

    pub fn visible_state_at(&self, cursor: Option<ReplayCursor>) -> Vec<&RecordedThought> {
        let end = cursor.map(|c| c.index + 1).unwrap_or(self.cursor.index + 1);
        self.thoughts.iter().take(end).collect()
    }

    /// Play through all checkpoints, calling `f` for each step
    pub fn replay_all<F>(&mut self, mut f: F)
    where
        F: FnMut(&RecordedThought),
    {
        self.reset();
        while let Some(thought) = self.current() {
            f(thought);
            if self.cursor.index + 1 >= self.thoughts.len() {
                break;
            }
            self.step_forward();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::RecordedThought;

    fn sample_thoughts() -> Vec<RecordedThought> {
        vec![
            RecordedThought::new("agent-1", "step 1"),
            RecordedThought::new("agent-1", "step 2"),
            RecordedThought::new("agent-1", "step 3"),
        ]
    }

    #[test]
    fn test_replay_initial_state() {
        let replayer = TraceReplayer::new(sample_thoughts());
        assert_eq!(replayer.cursor().index, 0);
        assert_eq!(replayer.current().unwrap().content, "step 1");
    }

    #[test]
    fn test_replay_step_forward() {
        let mut replayer = TraceReplayer::new(sample_thoughts());
        let next = replayer.step_forward().unwrap();
        assert_eq!(next.content, "step 2");
        assert_eq!(replayer.cursor().index, 1);
    }

    #[test]
    fn test_replay_step_backward() {
        let mut replayer = TraceReplayer::new(sample_thoughts());
        replayer.step_forward();
        replayer.step_forward();
        let prev = replayer.step_backward().unwrap();
        assert_eq!(prev.content, "step 2");
        assert_eq!(replayer.cursor().index, 1);
    }

    #[test]
    fn test_seek_by_checkpoint() {
        let thoughts = sample_thoughts();
        let target = thoughts[1].checkpoint_id.clone();
        let mut replayer = TraceReplayer::new(thoughts);
        let found = replayer.seek(&target).unwrap();
        assert_eq!(found.content, "step 2");
    }

    #[test]
    fn test_seek_nonexistent() {
        let mut replayer = TraceReplayer::new(sample_thoughts());
        assert!(replayer.seek("nonexistent").is_none());
    }

    #[test]
    fn test_reset_cursor() {
        let mut replayer = TraceReplayer::new(sample_thoughts());
        replayer.step_forward();
        replayer.step_forward();
        replayer.reset();
        assert_eq!(replayer.cursor().index, 0);
        assert_eq!(replayer.current().unwrap().content, "step 1");
    }

    #[test]
    fn test_replay_all() {
        let mut replayer = TraceReplayer::new(sample_thoughts());
        let mut visited = Vec::new();
        replayer.replay_all(|t| visited.push(t.content.clone()));
        assert_eq!(visited, vec!["step 1", "step 2", "step 3"]);
    }

    #[test]
    fn test_visible_state_at_cursor() {
        let replayer = TraceReplayer::new(sample_thoughts());
        let state = replayer.visible_state_at(Some(ReplayCursor::new(1)));
        assert_eq!(state.len(), 2);
        assert_eq!(state[0].content, "step 1");
        assert_eq!(state[1].content, "step 2");
    }
}
