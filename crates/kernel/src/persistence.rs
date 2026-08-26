use std::path::{Path, PathBuf};

use agentos_trace::RecordedThought;
use agentos_vault::{Vault, VaultEncryption};
use serde::{Deserialize, Serialize};

use crate::journal::RecordedSession;

use crate::error::{AgentError, AgentResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSnapshot {
    pub agent_id: String,
    pub thoughts: Vec<RecordedThought>,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSnapshot {
    pub secrets: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    pub captured_at_ms: u64,
}

#[derive(Debug)]
pub struct Persistence {
    data_dir: PathBuf,
}

impl Persistence {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub async fn ensure_dirs(&self) -> AgentResult<()> {
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create data dir: {e}")))?;
        tokio::fs::create_dir_all(self.data_dir.join("traces"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create traces dir: {e}")))?;
        tokio::fs::create_dir_all(self.data_dir.join("vault"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create vault dir: {e}")))?;
        tokio::fs::create_dir_all(self.data_dir.join("journals"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create journals dir: {e}")))?;
        Ok(())
    }

    pub async fn save_trace(
        &self,
        agent_id: &str,
        thoughts: &[RecordedThought],
    ) -> AgentResult<()> {
        let snapshot = TraceSnapshot {
            agent_id: agent_id.to_string(),
            thoughts: thoughts.to_vec(),
            captured_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };

        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| AgentError::Internal(format!("serialization error: {e}")))?;

        let path = self.trace_path(agent_id);

        // `tokio::fs::write` opens, writes and closes. The hand-rolled
        // `File::create` + `write_all` this replaces did not close: a
        // `tokio::fs::File` buffers and dispatches to the blocking pool, and
        // dropping it neither flushes nor waits, so `save_trace` could return
        // `Ok(())` with nothing on disk. `load_trace` then read an empty file
        // and reported the trace as corrupt.
        tokio::fs::write(&path, json.as_bytes())
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write trace: {e}")))?;

        Ok(())
    }

    pub async fn load_trace(&self, agent_id: &str) -> AgentResult<Vec<RecordedThought>> {
        let path = self.trace_path(agent_id);

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read trace file: {e}")))?;

        let snapshot: TraceSnapshot = serde_json::from_str(&content)
            .map_err(|e| AgentError::Internal(format!("cannot parse trace: {e}")))?;

        Ok(snapshot.thoughts)
    }

    fn vault_path(&self) -> PathBuf {
        self.data_dir.join("vault").join("secrets.enc")
    }

    /// Persist the vault encrypted with AES-256-GCM. Secrets are never
    /// written to disk in plaintext: encryption is a required argument,
    /// not an option.
    pub async fn save_vault(&self, vault: &Vault, encryption: &VaultEncryption) -> AgentResult<()> {
        let ciphertext = encryption
            .encrypt_json(vault)
            .map_err(|e| AgentError::Internal(format!("vault encryption error: {e}")))?;

        let path = self.vault_path();

        // Same unflushed-write defect as `save_trace`, and worse here:
        // `File::create` truncates `secrets.enc` before the write is
        // dispatched, so a lost buffer left the vault empty -- the previous
        // secrets destroyed, the new ones never stored, and `Ok(())`
        // returned either way.
        //
        // This closes the lost-buffer window, not the crash window: the write
        // is still in place rather than atomic, so a crash midway through
        // still truncates the file. Making it a temp-file-plus-rename is the
        // right next step for a secrets store, and is deliberately left out
        // of this fix rather than folded into it.
        tokio::fs::write(&path, &ciphertext)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write vault: {e}")))?;

        Ok(())
    }

    /// Load and decrypt the persisted vault. Returns an empty vault when no
    /// file exists yet; fails when the key does not match the file.
    pub async fn load_vault(&self, encryption: &VaultEncryption) -> AgentResult<Vault> {
        let path = self.vault_path();

        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Vault::new());
        }

        let ciphertext = tokio::fs::read(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read vault file: {e}")))?;

        let vault: Vault = encryption.decrypt_json(&ciphertext).map_err(|e| {
            AgentError::Internal(format!(
                "cannot decrypt vault (wrong AGENTOS_VAULT_KEY?): {e}"
            ))
        })?;

        Ok(vault)
    }

    /// Trace files are addressed by agent id, and an id is just a string on
    /// `AgentSpec` -- an embedder can take one from a manifest it did not
    /// author. Interpolating it raw let `../..` walk out of the data dir, so
    /// this goes through the same flattening `journal_path` uses. Both the
    /// read and the write side must call this, or a sanitized write becomes
    /// an unfindable read.
    fn trace_path(&self, agent_id: &str) -> PathBuf {
        self.data_dir
            .join("traces")
            .join(format!("{}.json", sanitize_file_id(agent_id)))
    }

    fn journal_path(&self, agent_id: &str) -> PathBuf {
        self.data_dir
            .join("journals")
            .join(format!("{}.json", sanitize_file_id(agent_id)))
    }

    /// Persist a recorded execution session (LLM exchanges + tool results).
    pub async fn save_journal(&self, session: &RecordedSession) -> AgentResult<()> {
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| AgentError::Internal(format!("journal serialization error: {e}")))?;

        let path = self.journal_path(&session.agent_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AgentError::Internal(format!("cannot create journals dir: {e}")))?;
        }

        tokio::fs::write(&path, json.as_bytes())
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write journal: {e}")))?;
        Ok(())
    }

    /// Load the recorded session for an agent, if one was journaled.
    pub async fn load_journal(&self, agent_id: &str) -> AgentResult<RecordedSession> {
        let path = self.journal_path(agent_id);
        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            AgentError::Internal(format!(
                "no recorded session for '{agent_id}' at {}: {e}",
                path.display()
            ))
        })?;
        serde_json::from_str(&content)
            .map_err(|e| AgentError::Internal(format!("cannot parse journal: {e}")))
    }

    pub async fn list_journals(&self) -> AgentResult<Vec<String>> {
        let dir = self.data_dir.join("journals");
        if !tokio::fs::try_exists(&dir).await.unwrap_or(false) {
            return Ok(Vec::new());
        }
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read journals dir: {e}")))?;

        let mut journals = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read entry: {e}")))?
        {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".json") {
                    journals.push(stem.to_string());
                }
            }
        }
        journals.sort();
        Ok(journals)
    }

    pub async fn list_traces(&self) -> AgentResult<Vec<String>> {
        let mut dir = tokio::fs::read_dir(self.data_dir.join("traces"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read traces dir: {e}")))?;

        let mut traces = Vec::new();
        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read entry: {e}")))?
        {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    traces.push(name.trim_end_matches(".json").to_string());
                }
            }
        }

        Ok(traces)
    }
}

/// Keep journal and trace filenames safe regardless of agent id contents.
/// Flattening is idempotent, so ids that come back out of `list_traces` as
/// filename stems still resolve through `load_trace`.
/// Flattening is lossy and therefore collides: `a.1`, `a_1` and `a:1` all
/// become `a_1` and share one file. That is acceptable only while ids are
/// opaque labels drawn from `[A-Za-z0-9_-]`, where the collapsed characters
/// cannot appear. Note that the HTTP layer's `is_valid_agent_id` is wider
/// than that -- it admits `.` and `:` -- so two ids that layer accepts as
/// distinct can land on the same journal. Containment (canonicalise, then
/// verify the result is inside the data dir) would preserve distinctness;
/// flattening trades it for a guarantee that is simpler to audit.
fn sanitize_file_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentos_trace::TraceRecorder;

    #[tokio::test]
    async fn test_persistence_save_and_load_trace() {
        let dir = std::env::temp_dir().join(format!("agentos_test_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let mut recorder = TraceRecorder::new();
        recorder.record_checkpoint("test-agent", "step 1");
        recorder.record_checkpoint("test-agent", "step 2");

        persist
            .save_trace("test-agent", recorder.thoughts())
            .await
            .unwrap();

        let loaded = persist.load_trace("test-agent").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].content, "step 1");
        assert_eq!(loaded[1].content, "step 2");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flattening_collides_distinct_ids() {
        // Documented limitation, pinned so it cannot change unnoticed. The
        // HTTP layer's `is_valid_agent_id` admits `.` and `:`, so these three
        // are distinct ids as far as the API is concerned, yet they share one
        // file here. Fixing that means containment instead of flattening --
        // a design change, not a tweak -- so this test exists to make the
        // trade explicit rather than to bless it.
        assert_eq!(sanitize_file_id("a.1"), "a_1");
        assert_eq!(sanitize_file_id("a:1"), "a_1");
        assert_eq!(sanitize_file_id("a_1"), "a_1");

        // Ids inside the intended character set pass through untouched, which
        // is why the collision stays theoretical for well-formed ids.
        assert_eq!(sanitize_file_id("agent-7_b"), "agent-7_b");
    }
    #[tokio::test]
    async fn trace_paths_stay_inside_the_data_dir() {
        // `save_journal` routes its agent id through `sanitize_file_id`; the
        // trace pair interpolated it straight into the path instead. An id is
        // just a string on `AgentSpec` and an embedder can take one from a
        // manifest it did not author, so a traversing id has to land inside
        // the data dir rather than wherever it points.
        //
        // The escape target carries the same uuid as the data dir, so this
        // test owns a path nothing else can collide with -- a shared target
        // could be failed by another run's leftovers, or, worse, cleaned up
        // by them and pass while the bug is present.
        let tag = uuid::Uuid::new_v4();
        let dir = std::env::temp_dir().join(format!("agentos_traversal_{tag}"));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let escaping_id = format!("../../escaped-{tag}");
        let escape_target = dir.parent().unwrap().join(format!("escaped-{tag}.json"));
        assert!(
            !escape_target.exists(),
            "test precondition: {} must not exist yet",
            escape_target.display()
        );

        let mut recorder = TraceRecorder::new();
        recorder.record_checkpoint("evil", "payload");

        persist
            .save_trace(&escaping_id, recorder.thoughts())
            .await
            .unwrap();

        let escaped = escape_target.exists();
        let _ = std::fs::remove_file(&escape_target);
        assert!(
            !escaped,
            "save_trace wrote outside the data dir, to {}",
            escape_target.display()
        );

        let written: Vec<_> = std::fs::read_dir(dir.join("traces"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            written,
            vec![format!("______escaped-{tag}.json")],
            "the traversing id should have been flattened into the traces dir"
        );

        // Sanitizing has to be applied identically on the read side, or a
        // safe write turns into a load that can never find its own file.
        let loaded = persist.load_trace(&escaping_id).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "payload");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One technique per entry rather than variations on one. The test above
    /// proves the fix works against `../..`; these exist so a future weakening
    /// fails here instead of shipping.
    fn traversal_payloads() -> Vec<(&'static str, String)> {
        vec![
            ("plain parent", "../escape".into()),
            ("nested parent", "../../../../../../escape".into()),
            ("windows separators", r"..\..\..\escape".into()),
            ("mixed separators", r"..\../..\escape".into()),
            ("absolute unix", "/etc/passwd".into()),
            ("absolute windows", r"C:\Windows\System32\escape".into()),
            ("unc path", r"\\server\share\escape".into()),
            ("url encoded", "%2e%2e%2f%2e%2e%2fescape".into()),
            ("double url encoded", "%252e%252e%252fescape".into()),
            ("overlong utf8 sequence", "..%c0%afescape".into()),
            (
                "unicode fullwidth solidus",
                "..\u{ff0f}..\u{ff0f}escape".into(),
            ),
            ("null byte truncation", "safe\u{0}../../escape".into()),
            ("trailing dots", "escape...".into()),
            ("current dir prefix", "./././escape".into()),
            ("embedded newline", "safe\n../../escape".into()),
            ("embedded tab", "safe\t../escape".into()),
            ("windows alternate data stream", "escape:hidden".into()),
            ("windows device name", "CON".into()),
            ("empty id", String::new()),
            ("only dots", "..".into()),
            ("single dot", ".".into()),
            ("home expansion", "~/escape".into()),
            ("shell variable", "$HOME/escape".into()),
            ("very long chain", "../".repeat(200) + "escape"),
            ("space padded", "  ../escape  ".into()),
            ("command separator", "escape;rm -rf /".into()),
        ]
    }

    fn files_under(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(files_under(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[tokio::test]
    async fn no_traversal_payload_escapes_the_traces_directory() {
        // Drives the public API and then asks the filesystem, rather than
        // asserting on what a helper returned: the claim under test is about
        // where bytes land.
        let tag = uuid::Uuid::new_v4();
        let root = std::env::temp_dir().join(format!("agentos_traversal_suite_{tag}"));
        let data_dir = root.join("data");
        let persist = Persistence::new(&data_dir);
        persist.ensure_dirs().await.unwrap();

        let canonical_traces = std::fs::canonicalize(data_dir.join("traces")).unwrap();

        let mut recorder = TraceRecorder::new();
        recorder.record_checkpoint("evil", "payload");

        let mut escapes: Vec<String> = Vec::new();

        for (label, id) in traversal_payloads() {
            // A write is allowed to fail; what it may never do is succeed at a
            // path outside `traces/`.
            let _ = persist.save_trace(&id, recorder.thoughts()).await;

            for entry in files_under(&root) {
                let parent = match entry.parent().map(std::fs::canonicalize) {
                    Some(Ok(p)) => p,
                    _ => continue,
                };
                if parent != canonical_traces {
                    escapes.push(format!("[{label}] id {id:?} produced {}", entry.display()));
                }
            }
        }

        // The sandbox's own parent, which a `../` payload aims straight at.
        let sibling = root.parent().unwrap().join("escape.json");
        if sibling.exists() {
            escapes.push(format!(
                "escaped the sandbox entirely: {}",
                sibling.display()
            ));
            let _ = std::fs::remove_file(&sibling);
        }

        let _ = std::fs::remove_dir_all(&root);
        assert!(
            escapes.is_empty(),
            "writes landed outside the traces directory:\n{}",
            escapes.join("\n")
        );
    }

    #[test]
    fn sanitized_output_cannot_express_a_path_component() {
        // The invariant containment rests on, checked over every Unicode
        // scalar value rather than a sample. If no input can produce a
        // separator, a dot or a colon, then `data_dir/traces/<out>.json` is
        // always exactly one level below `traces/` -- containment by
        // construction, with no canonicalise-then-check window to race.
        //
        // This guards a different axis from the escape test above: that one
        // catches the call site skipping the sanitizer, this one catches the
        // sanitizer itself being widened.
        let mut leaked: Vec<u32> = Vec::new();
        for cp in 0..=0x10_FFFF_u32 {
            let ch = match char::from_u32(cp) {
                Some(c) => c,
                None => continue,
            };
            let out = sanitize_file_id(&ch.to_string());
            if !out
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                leaked.push(cp);
                if leaked.len() >= 16 {
                    break;
                }
            }
        }
        assert!(
            leaked.is_empty(),
            "these code points survive outside [A-Za-z0-9_-]: {leaked:04X?}"
        );
    }

    #[test]
    fn sanitizing_is_idempotent_over_every_code_point() {
        // `list_traces` hands back filename stems and those stems go straight
        // back into `load_trace`. A second pass that changed anything would
        // break that round trip for exactly the ids that needed sanitizing.
        let mut unstable: Vec<u32> = Vec::new();
        for cp in 0..=0x10_FFFF_u32 {
            let ch = match char::from_u32(cp) {
                Some(c) => c,
                None => continue,
            };
            let once = sanitize_file_id(&ch.to_string());
            if sanitize_file_id(&once) != once {
                unstable.push(cp);
                if unstable.len() >= 16 {
                    break;
                }
            }
        }
        assert!(
            unstable.is_empty(),
            "sanitizing twice differs from sanitizing once for: {unstable:04X?}"
        );
    }

    #[tokio::test]
    async fn ids_that_were_always_legitimate_still_round_trip() {
        // Containment is only worth having if it leaves ordinary ids alone.
        let tag = uuid::Uuid::new_v4();
        let dir = std::env::temp_dir().join(format!("agentos_traversal_ok_{tag}"));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let mut recorder = TraceRecorder::new();
        recorder.record_checkpoint("a", "legitimate");

        for id in ["agent-1", "agent_2", "AGENT3", "a-b_c-9", "x"] {
            persist.save_trace(id, recorder.thoughts()).await.unwrap();
            let loaded = persist.load_trace(id).await.unwrap();
            assert_eq!(loaded.len(), 1, "id {id} did not round trip");
            assert_eq!(loaded[0].content, "legitimate");
            assert!(
                dir.join("traces").join(format!("{id}.json")).exists(),
                "id {id} was rewritten when it did not need to be"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn failed_writes_are_reported_rather_than_swallowed() {
        // The defect these two functions had was not only a lost write, it was
        // a lost write reported as success. Guarding the happy path alone would
        // not have caught it, so this pins the error path: with no
        // `ensure_dirs()` the target directories do not exist, every write
        // fails, and neither function may answer `Ok`.
        let dir = std::env::temp_dir().join(format!("agentos_test_nodir_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);

        let mut recorder = TraceRecorder::new();
        recorder.record_checkpoint("agent-1", "step 1");
        assert!(
            persist
                .save_trace("agent-1", recorder.thoughts())
                .await
                .is_err(),
            "save_trace reported success while the trace was not written"
        );

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-123");
        assert!(
            persist
                .save_vault(&vault, &VaultEncryption::new())
                .await
                .is_err(),
            "save_vault reported success while the secrets were not written"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_persistence_save_and_load_vault_encrypted() {
        let dir = std::env::temp_dir().join(format!("agentos_test_vault_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();
        let encryption = VaultEncryption::new();

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-123");
        vault.put("agent-1", "SECRET", "value");

        persist.save_vault(&vault, &encryption).await.unwrap();

        let loaded = persist.load_vault(&encryption).await.unwrap();
        assert!(loaded.has_secret("agent-1", "API_KEY"));
        assert!(loaded.has_secret("agent-1", "SECRET"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_vault_file_never_contains_plaintext_secrets() {
        let dir =
            std::env::temp_dir().join(format!("agentos_test_vault_plain_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();
        let encryption = VaultEncryption::new();

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-super-secret-value");
        persist.save_vault(&vault, &encryption).await.unwrap();

        let raw = std::fs::read(dir.join("vault").join("secrets.enc")).unwrap();
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(!raw_text.contains("sk-super-secret-value"));
        assert!(!raw_text.contains("API_KEY"));
        assert!(serde_json::from_slice::<serde_json::Value>(&raw).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_vault_load_with_wrong_key_fails() {
        let dir = std::env::temp_dir().join(format!(
            "agentos_test_vault_wrongkey_{}",
            uuid::Uuid::new_v4()
        ));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-123");
        persist
            .save_vault(&vault, &VaultEncryption::new())
            .await
            .unwrap();

        let result = persist.load_vault(&VaultEncryption::new()).await;
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_vault_load_missing_file_returns_empty() {
        let dir = std::env::temp_dir().join(format!(
            "agentos_test_vault_missing_{}",
            uuid::Uuid::new_v4()
        ));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let loaded = persist.load_vault(&VaultEncryption::new()).await.unwrap();
        assert!(loaded.list_keys("any-agent").is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_list_traces() {
        let dir = std::env::temp_dir().join(format!("agentos_test_list_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let recorder = TraceRecorder::new();
        persist
            .save_trace("agent-a", recorder.thoughts())
            .await
            .unwrap();
        persist
            .save_trace("agent-b", recorder.thoughts())
            .await
            .unwrap();

        let traces = persist.list_traces().await.unwrap();
        assert!(traces.contains(&"agent-a".to_string()));
        assert!(traces.contains(&"agent-b".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
