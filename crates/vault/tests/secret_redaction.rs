//! A secret must not be printable in cleartext by any formatting path.
//!
//! `Display` was redacted from the start, but `Debug` was derived, so `{:?}`
//! printed `inner` verbatim -- and `Vault` derives `Debug`, so one `{vault:?}`
//! dumped every stored credential. These tests fail if either path regresses.

use agentos_vault::{SecretValue, Vault};

const CANARY: &str = "sk-live-CANARY-do-not-log-9f3a";

#[test]
fn display_does_not_reveal_the_secret() {
    let secret = SecretValue::new(CANARY);
    assert!(!format!("{secret}").contains(CANARY));
}

#[test]
fn debug_does_not_reveal_the_secret() {
    let secret = SecretValue::new(CANARY);
    let rendered = format!("{secret:?}");
    assert!(
        !rendered.contains(CANARY),
        "Debug leaked the secret: {rendered}"
    );
    // Still useful for debugging: the field is present, the value is not.
    assert!(rendered.contains("access_count"));
}

#[test]
fn debug_on_the_whole_vault_does_not_reveal_stored_secrets() {
    let mut vault = Vault::new();
    vault.put("agent-1", "OPENAI_API_KEY", CANARY);
    let rendered = format!("{vault:?}");
    assert!(
        !rendered.contains(CANARY),
        "Vault Debug leaked a stored secret: {rendered}"
    );
}
