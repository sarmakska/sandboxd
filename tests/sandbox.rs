//! Integration tests for the sandbox guarantees.
//!
//! Each test exercises one of the contract points in the threat model against
//! a real WAT fixture, compiled and run through the public library API.

use std::time::Duration;

use sandboxd::{HostAbi, Limits, Sandbox, SandboxError, Value};

const INFINITE_LOOP: &str = include_str!("../fixtures/infinite_loop.wat");
const MEMORY_BOMB: &str = include_str!("../fixtures/memory_bomb.wat");
const DISALLOWED_IMPORT: &str = include_str!("../fixtures/disallowed_import.wat");
const WELL_BEHAVED: &str = include_str!("../fixtures/well_behaved.wat");
const LOGGER: &str = include_str!("../fixtures/logger.wat");

/// Fuel exhaustion terminates an infinite loop deterministically, even with a
/// long timeout that would never fire.
#[test]
fn fuel_exhaustion_terminates() {
    let sandbox = Sandbox::deny_all().unwrap();
    let limits = Limits::new(1_000_000, Duration::from_secs(60), 1 << 20);

    let err = sandbox
        .run(INFINITE_LOOP.as_bytes(), "run", &[], &limits)
        .unwrap_err();

    match err {
        SandboxError::FuelExhausted { budget } => assert_eq!(budget, 1_000_000),
        other => panic!("expected FuelExhausted, got {other:?}"),
    }
}

/// The epoch timeout terminates an infinite loop even when the fuel budget is
/// effectively unlimited.
#[test]
fn epoch_timeout_terminates() {
    let sandbox = Sandbox::deny_all().unwrap();
    // A huge fuel budget so fuel cannot be the cause; a short wall-clock
    // deadline so the watchdog is what stops the guest.
    let limits = Limits::new(u64::MAX, Duration::from_millis(100), 1 << 20);

    let start = std::time::Instant::now();
    let err = sandbox
        .run(INFINITE_LOOP.as_bytes(), "run", &[], &limits)
        .unwrap_err();
    let elapsed = start.elapsed();

    match err {
        SandboxError::Timeout { millis } => assert_eq!(millis, 100),
        other => panic!("expected Timeout, got {other:?}"),
    }
    // It should have stopped near the deadline, not run for many seconds.
    assert!(
        elapsed < Duration::from_secs(5),
        "timeout took too long: {elapsed:?}"
    );
}

/// The memory cap is enforced: an over-allocating module is stopped.
#[test]
fn memory_cap_enforced() {
    let sandbox = Sandbox::deny_all().unwrap();
    // Four mebibytes of memory, plenty of fuel, generous timeout. The module
    // tries to grow past the cap and traps.
    let limits = Limits::new(1_000_000_000, Duration::from_secs(60), 4 * 1024 * 1024);

    let err = sandbox
        .run(MEMORY_BOMB.as_bytes(), "run", &[], &limits)
        .unwrap_err();

    match err {
        SandboxError::MemoryLimitExceeded { limit } => assert_eq!(limit, 4 * 1024 * 1024),
        other => panic!("expected MemoryLimitExceeded, got {other:?}"),
    }
}

/// A disallowed import is rejected at instantiation, before any guest code
/// executes, and the error names the offending import.
#[test]
fn disallowed_import_rejected() {
    let sandbox = Sandbox::deny_all().unwrap();
    let limits = Limits::default();

    let err = sandbox
        .run(DISALLOWED_IMPORT.as_bytes(), "run", &[], &limits)
        .unwrap_err();

    match err {
        SandboxError::DisallowedImport { module, name } => {
            assert_eq!(module, "env");
            assert_eq!(name, "secret");
        }
        other => panic!("expected DisallowedImport, got {other:?}"),
    }
}

/// The logger fixture is rejected when the log capability is not granted, even
/// though `host::log` is the one capability the host knows about.
#[test]
fn log_import_denied_by_default() {
    let sandbox = Sandbox::deny_all().unwrap();
    let limits = Limits::default();

    let err = sandbox
        .run(LOGGER.as_bytes(), "run", &[], &limits)
        .unwrap_err();

    match err {
        SandboxError::DisallowedImport { module, name } => {
            assert_eq!(module, "host");
            assert_eq!(name, "log");
        }
        other => panic!("expected DisallowedImport, got {other:?}"),
    }
}

/// When the embedder grants the log capability, the import works and the host
/// captures exactly what the guest emitted.
#[test]
fn allowed_import_works() {
    let (host, sink) = HostAbi::deny_all().allow_log();
    let sandbox = Sandbox::new(host).unwrap();
    let limits = Limits::default();

    let out = sandbox
        .run(LOGGER.as_bytes(), "run", &[], &limits)
        .expect("logger should run when log is allowed");

    assert!(out.values.is_empty());
    let lines = sink.lock().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], "hello from the guest");
}

/// A well-behaved pure module returns the expected value.
#[test]
fn well_behaved_returns_value() {
    let sandbox = Sandbox::deny_all().unwrap();
    let limits = Limits::default();

    let out = sandbox
        .run(
            WELL_BEHAVED.as_bytes(),
            "add",
            &[Value::I32(2), Value::I32(40)],
            &limits,
        )
        .unwrap();

    assert_eq!(out.values, vec![Value::I32(42)]);
    assert!(out.fuel_consumed.is_some());
}

/// A pure module is deterministic: the same inputs produce the same outputs
/// and the same fuel consumption across runs and across fresh sandboxes.
#[test]
fn pure_module_is_deterministic() {
    let limits = Limits::default();

    let run_once = || {
        let sandbox = Sandbox::deny_all().unwrap();
        sandbox
            .run(WELL_BEHAVED.as_bytes(), "fib", &[Value::I32(20)], &limits)
            .unwrap()
    };

    let first = run_once();
    let second = run_once();
    let third = run_once();

    // fib(20) iteratively: 6765.
    assert_eq!(first.values, vec![Value::I32(6765)]);
    assert_eq!(first.values, second.values);
    assert_eq!(second.values, third.values);
    // Fuel consumption is identical for identical pure runs.
    assert_eq!(first.fuel_consumed, second.fuel_consumed);
    assert_eq!(second.fuel_consumed, third.fuel_consumed);
}

/// Invoking a missing export produces a clean typed error rather than a panic.
#[test]
fn missing_export_is_reported() {
    let sandbox = Sandbox::deny_all().unwrap();
    let limits = Limits::default();

    let err = sandbox
        .run(WELL_BEHAVED.as_bytes(), "does_not_exist", &[], &limits)
        .unwrap_err();

    assert!(matches!(err, SandboxError::Export(_)));
}

/// A parameter-arity mismatch is reported cleanly.
#[test]
fn signature_mismatch_is_reported() {
    let sandbox = Sandbox::deny_all().unwrap();
    let limits = Limits::default();

    let err = sandbox
        .run(WELL_BEHAVED.as_bytes(), "add", &[Value::I32(1)], &limits)
        .unwrap_err();

    assert!(matches!(err, SandboxError::Export(_)));
}

/// Invalid module bytes produce InvalidModule rather than a panic.
#[test]
fn invalid_module_is_reported() {
    let sandbox = Sandbox::deny_all().unwrap();
    let limits = Limits::default();

    let err = sandbox
        .run(b"this is not wasm", "run", &[], &limits)
        .unwrap_err();

    assert!(matches!(err, SandboxError::InvalidModule(_)));
}
