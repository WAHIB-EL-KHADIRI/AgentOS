use crate::recorder::RecordedThought;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceDiff {
    pub checkpoint_id: String,
    pub left: Option<String>,
    pub right: Option<String>,
    pub left_metadata: Option<std::collections::HashMap<String, String>>,
    pub right_metadata: Option<std::collections::HashMap<String, String>>,
}

pub fn diff_traces(left: &[RecordedThought], right: &[RecordedThought]) -> Vec<TraceDiff> {
    let max_len = left.len().max(right.len());
    let mut diffs = Vec::new();

    for index in 0..max_len {
        let left_item = left.get(index);
        let right_item = right.get(index);

        let left_content = left_item.map(|item| &item.content);
        let right_content = right_item.map(|item| &item.content);

        let changed = left_content != right_content
            || left_item.map(|l| &l.metadata) != right_item.map(|r| &r.metadata);

        if changed {
            diffs.push(TraceDiff {
                checkpoint_id: left_item
                    .or(right_item)
                    .map(|item| item.checkpoint_id.clone())
                    .unwrap_or_else(|| format!("index_{index}")),
                left: left_item.map(|item| item.content.clone()),
                right: right_item.map(|item| item.content.clone()),
                left_metadata: left_item.map(|item| item.metadata.clone()),
                right_metadata: right_item.map(|item| item.metadata.clone()),
            });
        }
    }

    diffs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::RecordedThought;

    fn thought(content: &str, id: &str) -> RecordedThought {
        RecordedThought {
            checkpoint_id: id.into(),
            agent_id: "agent-1".into(),
            content: content.into(),
            timestamp_ms: 1000,
            parent_checkpoint_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_identical_traces_no_diffs() {
        let left = vec![thought("a", "1"), thought("b", "2")];
        let right = vec![thought("a", "1"), thought("b", "2")];
        let diffs = diff_traces(&left, &right);
        assert_eq!(diffs.len(), 0);
    }

    #[test]
    fn test_different_content_produces_diff() {
        let left = vec![thought("a", "1")];
        let right = vec![thought("b", "1")];
        let diffs = diff_traces(&left, &right);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].left.as_deref(), Some("a"));
        assert_eq!(diffs[0].right.as_deref(), Some("b"));
    }

    #[test]
    fn test_uneven_lengths() {
        let left = vec![thought("a", "1"), thought("b", "2")];
        let right = vec![thought("a", "1")];
        let diffs = diff_traces(&left, &right);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].right, None);
    }

    #[test]
    fn test_empty_traces() {
        let diffs = diff_traces(&[], &[]);
        assert_eq!(diffs.len(), 0);
    }
}
