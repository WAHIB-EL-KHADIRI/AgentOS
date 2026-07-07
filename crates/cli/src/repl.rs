use std::sync::Arc;

use agentos_kernel::{AgentOSSystem, AgentSpec, RuntimeConfig};
use rustyline::{error::ReadlineError, Editor};
use tracing::error;

enum ReplCommand {
    Spawn {
        name: String,
    },
    Stop {
        id: String,
    },
    List,
    Thought {
        id: String,
        content: String,
    },
    Logs {
        id: String,
    },
    Trace {
        id: String,
    },
    Memory {
        id: String,
        content: String,
    },
    Search {
        id: String,
        query: String,
    },
    Secret {
        id: String,
        key: String,
        value: Option<String>,
    },
    Status {
        id: String,
    },
    Help,
    Exit,
}

fn parse_input(input: &str) -> Option<ReplCommand> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let parts: Vec<&str> = input.splitn(4, ' ').collect();
    let cmd = parts[0].to_lowercase();

    match cmd.as_str() {
        "spawn" => {
            let name = parts.get(1)?.to_string();
            Some(ReplCommand::Spawn { name })
        }
        "stop" => {
            let id = parts.get(1)?.to_string();
            Some(ReplCommand::Stop { id })
        }
        "list" | "ls" => Some(ReplCommand::List),
        "thought" => {
            let id = parts.get(1)?.to_string();
            let content = parts.get(2).copied().unwrap_or("Agent thought").to_string();
            Some(ReplCommand::Thought { id, content })
        }
        "logs" | "log" => {
            let id = parts.get(1)?.to_string();
            Some(ReplCommand::Logs { id })
        }
        "trace" => {
            let id = parts.get(1)?.to_string();
            Some(ReplCommand::Trace { id })
        }
        "memory" | "mem" => {
            let id = parts.get(1)?.to_string();
            let content = parts.get(2).copied().unwrap_or("Memory data").to_string();
            Some(ReplCommand::Memory { id, content })
        }
        "search" => {
            let id = parts.get(1)?.to_string();
            let query = parts.get(2).copied().unwrap_or("").to_string();
            Some(ReplCommand::Search { id, query })
        }
        "secret" => {
            let id = parts.get(1)?.to_string();
            let key = parts.get(2).map(|s| s.to_string())?;
            let value = parts.get(3).map(|s| s.to_string());
            Some(ReplCommand::Secret { id, key, value })
        }
        "status" => {
            let id = parts.get(1)?.to_string();
            Some(ReplCommand::Status { id })
        }
        "help" | "?" => Some(ReplCommand::Help),
        "exit" | "quit" | "q" => Some(ReplCommand::Exit),
        _ => {
            println!("Unknown command: {cmd}. Type 'help' for available commands.");
            None
        }
    }
}

fn print_help() {
    println!("AgentOS REPL Help");
    println!("{}", "-".repeat(48));
    println!(" spawn <name> [prompt]   Spawn a new agent");
    println!(" stop <id>               Stop a running agent");
    println!(" list | ls               List all agents");
    println!(" thought <id> <text>     Record a thought");
    println!(" logs <id>               View agent logs");
    println!(" trace <id>              View agent trace");
    println!(" memory <id> <text>      Store agent memory");
    println!(" search <id> <query>     Search agent memories");
    println!(" secret <id> <key> [val] Get or set a secret");
    println!(" status <id>             Show agent status");
    println!(" help                    Show this help");
    println!(" exit | quit             Exit the REPL");
    println!("{}", "-".repeat(48));
}

pub async fn run_repl() -> anyhow::Result<()> {
    let config = RuntimeConfig::default();
    let system = Arc::new(AgentOSSystem::with_config(config));

    println!();
    println!("AgentOS Interactive REPL");
    println!("{}", "-".repeat(48));
    println!("AI Agent Operating Layer v0.1.0");
    println!();
    print_help();
    println!();

    let mut rl = Editor::<(), rustyline::history::FileHistory>::new()?;
    rl.load_history(".agentos_repl_history").ok();

    loop {
        let readline = rl.readline("agentOS> ");
        match readline {
            Ok(line) => {
                let _ = rl.add_history_entry(line.as_str());
                if let Err(e) = rl.save_history(".agentos_repl_history") {
                    eprintln!("Warning: failed to save history: {e}");
                }

                match parse_input(&line) {
                    Some(ReplCommand::Exit) => {
                        println!("Shutting down all agents...");
                        system.shutdown_all().await;
                        println!("Goodbye!");
                        break;
                    }
                    Some(ReplCommand::Help) => {
                        print_help();
                    }
                    Some(ReplCommand::List) => {
                        let handles = system.supervisor.list().await;
                        if handles.is_empty() {
                            println!("  No agents running.");
                        } else {
                            println!("  Agents ({})", handles.len());
                            println!("  {}", "-".repeat(48));
                            for h in &handles {
                                let state = h.state().await;
                                let status = match &state {
                                    agentos_kernel::AgentState::Running => "running",
                                    agentos_kernel::AgentState::Stopped => "stopped",
                                    agentos_kernel::AgentState::Failed(_) => "failed",
                                    agentos_kernel::AgentState::Degraded(_) => "degraded",
                                    agentos_kernel::AgentState::Created => "created",
                                };
                                println!(
                                    "  [{:<8}] {} restarts: {}",
                                    status,
                                    h.id,
                                    h.restart_count()
                                );
                            }
                        }
                    }
                    Some(ReplCommand::Spawn { name }) => {
                        let id = format!("agent_{}", name.replace('-', "_"));
                        let spec = AgentSpec::new(&id, &name);
                        match system.spawn_agent(spec).await {
                            Ok(_) => {
                                println!("  [ok] Agent '{id}' spawned successfully");
                            }
                            Err(e) => {
                                println!("  [error] Failed to spawn agent: {e}");
                            }
                        }
                    }
                    Some(ReplCommand::Stop { id }) => match system.supervisor.stop(&id).await {
                        Ok(_) => println!("  [ok] Agent '{id}' stopped"),
                        Err(e) => println!("  [error] Failed to stop agent: {e}"),
                    },
                    Some(ReplCommand::Thought { id, content }) => {
                        let thought_id = system.record_thought(&id, &content).await;
                        println!("  [ok] Thought recorded: {thought_id}");
                    }
                    Some(ReplCommand::Logs { id }) => {
                        let logs = system.get_logs(&id, 20).await;
                        if logs.is_empty() {
                            println!("  No logs for '{id}'");
                        } else {
                            println!("  Logs for '{id}':");
                            println!("  {}", "-".repeat(48));
                            for log in &logs {
                                println!(
                                    "  [{}] {} {}",
                                    log.timestamp_ms, log.event_type, log.message
                                );
                            }
                        }
                    }
                    Some(ReplCommand::Trace { id }) => {
                        let trace = system.trace_recorder.read().await;
                        let thoughts = trace.thoughts_for_agent(&id);
                        if thoughts.is_empty() {
                            println!("  No trace data for '{id}'");
                        } else {
                            println!("  Trace for '{id}':");
                            println!("  {}", "-".repeat(48));
                            for t in &thoughts {
                                println!("  [{}] {}", t.checkpoint_id, t.content);
                            }
                        }
                    }
                    Some(ReplCommand::Memory { id, content }) => {
                        match system.store_memory(&id, &content).await {
                            Ok(mem_id) => println!("  [ok] Memory stored: {mem_id}"),
                            Err(e) => println!("  [error] Failed to store memory: {e}"),
                        }
                    }
                    Some(ReplCommand::Search { id, query }) => {
                        match system.search_memory(&id, &query, 5).await {
                            Ok(results) => {
                                if results.is_empty() {
                                    println!("  No results for query '{query}'");
                                } else {
                                    println!("  Memory search results:");
                                    println!("  {}", "-".repeat(48));
                                    for r in &results {
                                        println!("  [{:.8}] {}", r.id, r.content);
                                    }
                                }
                            }
                            Err(e) => println!("  [error] Search failed: {e}"),
                        }
                    }
                    Some(ReplCommand::Secret { id, key, value }) => {
                        if let Some(val) = value {
                            system.set_secret(&id, &key, &val).await;
                            println!("  [ok] Secret '{key}' set for '{id}'");
                        } else {
                            match system.get_secret(&id, &key).await {
                                Some(_) => println!("  [ok] Secret '{key}' exists for '{id}'"),
                                None => println!("  No secret '{key}' found for '{id}'"),
                            }
                        }
                    }
                    Some(ReplCommand::Status { id }) => {
                        if let Some(handle) = system.supervisor.get(&id).await {
                            let state = handle.state().await;
                            println!("  Agent: {}", handle.id);
                            println!("  State: {state}");
                            println!("  Restarts: {}", handle.restart_count());
                            println!("  Spec: {}", handle.spec().name);
                        } else {
                            println!("  Agent '{id}' not found");
                        }
                    }
                    None => {}
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!();
                system.shutdown_all().await;
                break;
            }
            Err(err) => {
                error!("Readline error: {err}");
                break;
            }
        }
    }

    Ok(())
}
