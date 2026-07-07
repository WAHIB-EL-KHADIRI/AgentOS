CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_agent_id ON memories(agent_id);

