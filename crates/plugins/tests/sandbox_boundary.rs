//! Sandbox boundary regression suite.
//!
//! These are the guarantees the plugin system exists to provide: untrusted
//! WASM runs with bounded CPU, bounded memory, and no reach into the host
//! beyond a single logging call. None of that was covered -- the crate's tests
//! constructed a runtime and listed plugins, which says nothing about what a
//! hostile module can do once it is inside.
//!
//! Everything here drives the public API (`load_wasm`, `invoke`), so the tests
//! exercise the same path a real plugin takes. Modules are written in WAT and
//! compiled by wasmtime at run time: no build step, no checked-in binaries,
//! and the adversarial intent of each module is legible in the diff.
//!
//! The point is to be able to answer "does upgrading wasmtime still contain a
//! plugin?" with a test run instead of a changelog reading.

use agentos_plugins::PluginRuntime;

/// Denial here is structural rather than a blocklist: `invoke` builds a fresh
/// `Linker` and adds exactly one function to it, so a module importing
/// anything else cannot be instantiated. These cases pin that property from
/// the outside, one realistic capability at a time.
async fn assert_import_is_denied(case: &str, wat: &str) {
    let runtime = PluginRuntime::new().expect("engine");
    runtime
        .load_wasm(case, wat.as_bytes())
        .await
        .unwrap_or_else(|e| panic!("{case}: module should compile, only linking may fail: {e}"));

    let result = runtime.invoke(case, "run").await;

    assert!(
        result.is_err(),
        "{case}: the sandbox linked an import it should not provide -- a plugin can reach the host"
    );
}

#[tokio::test]
async fn wasi_is_not_available_to_plugins() {
    // fd_write is the whole of WASI's stdio surface and the first thing a
    // wasi-targeting toolchain emits. If this links, the module was built
    // against a WASI-enabled linker and every other WASI call is reachable.
    assert_import_is_denied(
        "wasi-fd-write",
        r#"(module
             (import "wasi_snapshot_preview1" "fd_write"
               (func $fd_write (param i32 i32 i32 i32) (result i32)))
             (func (export "run") (result i32)
               (call $fd_write (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0))))"#,
    )
    .await;
}

#[tokio::test]
async fn filesystem_access_is_denied() {
    assert_import_is_denied(
        "wasi-path-open",
        r#"(module
             (import "wasi_snapshot_preview1" "path_open"
               (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
             (func (export "run") (result i32)
               (call $path_open (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0)
                                (i32.const 0) (i64.const 0) (i64.const 0) (i32.const 0)
                                (i32.const 0))))"#,
    )
    .await;
}

#[tokio::test]
async fn network_access_is_denied() {
    assert_import_is_denied(
        "wasi-sock-accept",
        r#"(module
             (import "wasi_snapshot_preview1" "sock_accept"
               (func $sock_accept (param i32 i32 i32) (result i32)))
             (func (export "run") (result i32)
               (call $sock_accept (i32.const 0) (i32.const 0) (i32.const 0))))"#,
    )
    .await;
}

#[tokio::test]
async fn environment_access_is_denied() {
    // Host environment is where credentials live -- AGENTOS_API_TOKEN and
    // provider keys are all read from it, so this one is not hypothetical.
    assert_import_is_denied(
        "wasi-environ-get",
        r#"(module
             (import "wasi_snapshot_preview1" "environ_get"
               (func $environ_get (param i32 i32) (result i32)))
             (func (export "run") (result i32)
               (call $environ_get (i32.const 0) (i32.const 0))))"#,
    )
    .await;
}

#[tokio::test]
async fn arbitrary_host_modules_are_denied() {
    // Guards the shape of the rule, not one namespace: nothing is reachable
    // merely because a module asks for it by a plausible name.
    assert_import_is_denied(
        "invented-host",
        r#"(module
             (import "host" "exec" (func $exec (param i32) (result i32)))
             (func (export "run") (result i32) (call $exec (i32.const 0))))"#,
    )
    .await;
}

#[tokio::test]
async fn the_log_host_function_is_the_one_thing_that_links() {
    // The negative tests above are only meaningful if instantiation can
    // succeed at all -- otherwise they would pass against a runtime that
    // links nothing and is simply broken.
    let runtime = PluginRuntime::new().expect("engine");
    let wat = r#"(module
                   (import "env" "log" (func $log (param i32 i32)))
                   (func (export "run") (result i32)
                     (call $log (i32.const 0) (i32.const 0))
                     (i32.const 7)))"#;

    runtime
        .load_wasm("allowed-log", wat.as_bytes())
        .await
        .expect("load");

    assert_eq!(
        runtime
            .invoke("allowed-log", "run")
            .await
            .expect("env::log must link"),
        "7"
    );
}

#[tokio::test]
async fn an_infinite_loop_is_stopped_by_fuel() {
    // Without fuel metering this test does not fail -- it hangs, taking the
    // test run with it. That is exactly the production failure being guarded
    // against: one plugin spinning forever inside the host process.
    let runtime = PluginRuntime::new().expect("engine");
    let wat = r#"(module (func (export "run") (loop $l (br $l))))"#;

    runtime
        .load_wasm("spinner", wat.as_bytes())
        .await
        .expect("load");

    let err = runtime
        .invoke("spinner", "run")
        .await
        .expect_err("an unbounded loop must not be allowed to run to completion");

    assert!(
        format!("{err}").to_lowercase().contains("fuel"),
        "expected fuel exhaustion, got: {err}"
    );
}

#[tokio::test]
async fn a_trap_is_contained_and_reported() {
    // A misbehaving plugin must surface as an error value, never as a panic
    // that unwinds through the host.
    let runtime = PluginRuntime::new().expect("engine");
    let wat = r#"(module (func (export "run") unreachable))"#;

    runtime
        .load_wasm("trapper", wat.as_bytes())
        .await
        .expect("load");

    assert!(
        runtime.invoke("trapper", "run").await.is_err(),
        "a trap must be returned as an error, not propagated"
    );
}

#[tokio::test]
async fn memory_growth_beyond_the_limit_is_refused() {
    // 1600 pages is 100 MiB against a 1 MiB ceiling. `memory.grow` answers -1
    // when the limiter refuses, so a passing assertion here means the store
    // limiter actually ran -- not merely that it was configured.
    let runtime = PluginRuntime::with_max_memory(1024 * 1024).expect("engine");
    let wat = r#"(module
                   (memory 1)
                   (func (export "run") (result i32)
                     (memory.grow (i32.const 1600))))"#;

    runtime
        .load_wasm("grower", wat.as_bytes())
        .await
        .expect("load");

    assert_eq!(
        runtime
            .invoke("grower", "run")
            .await
            .expect("grow returns, it does not trap"),
        "-1",
        "the store limiter did not refuse growth past its ceiling"
    );
}

#[tokio::test]
async fn a_declared_memory_over_the_limit_cannot_instantiate() {
    // The other half of the memory boundary: refusing growth is not enough if
    // a module can simply declare an oversized memory up front. 64 pages is
    // 4 MiB against the same 1 MiB ceiling.
    let runtime = PluginRuntime::with_max_memory(1024 * 1024).expect("engine");
    let wat = r#"(module (memory 64) (func (export "run")))"#;

    runtime
        .load_wasm("big-memory", wat.as_bytes())
        .await
        .expect("load");

    assert!(
        runtime.invoke("big-memory", "run").await.is_err(),
        "a 4 MiB declared memory instantiated under a 1 MiB ceiling"
    );
}
