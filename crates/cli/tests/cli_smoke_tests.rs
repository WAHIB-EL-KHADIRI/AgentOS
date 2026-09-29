use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn agentos_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_agentOS"))
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "agentos_cli_smoke_{name}_{}_{}",
        std::process::id(),
        now
    ))
}

fn run_agentos(args: &[&str], cwd: &Path, home: &Path) -> Output {
    let mut command = agentos_command();
    command
        .args(args)
        .current_dir(cwd)
        .env("AGENTOS_STATE_BACKEND", "json")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("NO_COLOR", "1");
    command.output().expect("failed to run agentOS")
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn help_renders_core_commands() {
    let cwd = temp_dir("help_cwd");
    let home = temp_dir("help_home");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    let output = run_agentos(&["--help"], &cwd, &home);
    let text = output_text(&output);
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);

    assert!(output.status.success(), "{text}");
    assert!(text.contains("local runtime layer for AI agents"));
    assert!(text.contains("repl"));
    assert!(text.contains("state"));
    assert!(text.contains("marketplace"));
}

#[test]
fn state_inspect_json_handles_missing_state_without_writing() {
    let cwd = temp_dir("inspect_cwd");
    let home = temp_dir("inspect_home");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    let output = run_agentos(&["state", "inspect", "--json"], &cwd, &home);
    let text = output_text(&output);
    let state_path = cwd.join(".agentos").join("cli-state.json");
    let state_exists = state_path.exists();
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);

    assert!(output.status.success(), "{text}");
    assert!(text.contains("\"valid\": false"));
    assert!(text.contains("\"agents\": 0"));
    assert!(!state_exists, "inspect must stay read-only");
}

#[test]
fn marketplace_list_empty_registry_is_friendly() {
    let cwd = temp_dir("marketplace_cwd");
    let home = temp_dir("marketplace_home");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    let output = run_agentos(&["marketplace", "list"], &cwd, &home);
    let text = output_text(&output);
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);

    assert!(output.status.success(), "{text}");
    assert!(text.contains("No plugins installed."));
    assert!(text.contains("agentOS marketplace install <name>"));
}

#[test]
fn fork_unknown_checkpoint_reports_clear_error() {
    let cwd = temp_dir("fork_cwd");
    let home = temp_dir("fork_home");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    let output = run_agentos(
        &[
            "fork",
            "--from",
            "ckpt_test",
            "--prompt",
            "try another path",
        ],
        &cwd,
        &home,
    );
    let text = output_text(&output);

    assert!(!output.status.success(), "{text}");
    assert!(text.contains("not found in any recorded session"), "{text}");
    assert!(text.contains("agentOS replay --session"), "{text}");

    // Without any selector the command explains its usage.
    let no_args = run_agentos(&["fork"], &cwd, &home);
    let no_args_text = output_text(&no_args);
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);

    assert!(!no_args.status.success(), "{no_args_text}");
    assert!(no_args_text.contains("--session"), "{no_args_text}");
}

fn replay_example_fixture(name: &str, tamper: bool) -> (std::path::PathBuf, std::path::PathBuf) {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    let journals = cwd.join("data").join("journals");
    std::fs::create_dir_all(&journals).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/replay/journals/demo_agent.json");
    let mut journal = std::fs::read_to_string(source).unwrap();
    if tamper {
        assert!(journal.contains("\"v2:"), "fixture layout changed");
        journal = journal.replacen("\"v2:", "\"v2:0", 1);
    }
    std::fs::write(journals.join("demo_agent.json"), journal).unwrap();
    std::fs::write(cwd.join("agentos.toml"), "data_dir = \"data\"\n").unwrap();
    (cwd, home)
}

#[test]
fn replay_of_the_shipped_example_exits_zero() {
    let (cwd, home) = replay_example_fixture("replay_ok", false);

    let output = run_agentos(
        &[
            "replay",
            "--session",
            "demo_agent",
            "--config",
            "agentos.toml",
        ],
        &cwd,
        &home,
    );
    let text = output_text(&output);
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);

    assert!(output.status.success(), "{text}");
    assert!(text.contains("no drift against the recording"), "{text}");
}

#[test]
fn replay_with_drift_exits_nonzero() {
    let (cwd, home) = replay_example_fixture("replay_drift", true);

    let output = run_agentos(
        &[
            "replay",
            "--session",
            "demo_agent",
            "--config",
            "agentos.toml",
        ],
        &cwd,
        &home,
    );
    let text = output_text(&output);
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);

    assert!(!output.status.success(), "{text}");
    assert!(text.contains("drift detected"), "{text}");
}
