# Examples and Recipes

Copy-paste snippets for the things embedders actually do. Each one is complete enough to drop into a project. The library surface is `Sandbox`, `Limits`, `HostAbi`, `Value`, `RunOutput`, `SandboxError`; see the [API Reference](API-Reference) for full signatures.

## Run a pure function and read the result

```rust
use std::time::Duration;
use sandboxd::{Sandbox, Limits, Value};

let wat = r#"(module (func (export "add") (param i32 i32) (result i32)
    local.get 0 local.get 1 i32.add))"#;

let sandbox = Sandbox::deny_all()?;
let limits = Limits::new(1_000_000, Duration::from_millis(500), 1 << 20);
let out = sandbox.run(wat.as_bytes(), "add", &[Value::I32(2), Value::I32(40)], &limits)?;

assert_eq!(out.values, vec![Value::I32(42)]);
println!("fuel: {:?}", out.fuel_consumed);
# Ok::<(), sandboxd::SandboxError>(())
```

## Reuse one sandbox for many runs

The engine is expensive; the store is cheap. Build the `Sandbox` once and call `run` repeatedly.

```rust
use std::time::Duration;
use sandboxd::{Sandbox, Limits, Value};

let sandbox = Sandbox::deny_all()?;
let limits = Limits::new(5_000_000, Duration::from_secs(1), 4 << 20);

for n in [10, 20, 30] {
    let out = sandbox.run(WAT, "fib", &[Value::I32(n)], &limits)?;
    println!("fib({n}) = {:?}, fuel {:?}", out.values, out.fuel_consumed);
}
# const WAT: &[u8] = b"";
# Ok::<(), sandboxd::SandboxError>(())
```

Each `run` builds its own store, so the loop iterations are fully isolated from each other.

## Grant the log capability and audit what the guest emitted

```rust
use sandboxd::{HostAbi, Sandbox, Limits};

let (host, sink) = HostAbi::deny_all().allow_log();
let sandbox = Sandbox::new(host)?;
let limits = Limits::default();

sandbox.run(LOGGER_WAT, "run", &[], &limits)?;

for line in sink.lock().unwrap().iter() {
    println!("guest said: {line}");
}
# const LOGGER_WAT: &[u8] = b"";
# Ok::<(), sandboxd::SandboxError>(())
```

The guest must export its memory as `memory` and call `host::log` with a pointer and length into it. The host copies the string out with bounds checking; see [Host ABI](Host-ABI).

## Profile a workload, then lock in a budget

```rust
use std::time::Duration;
use sandboxd::{Sandbox, Limits};

let sandbox = Sandbox::deny_all()?;

// Loose limits to observe the true cost.
let loose = Limits::new(u64::MAX / 2, Duration::from_secs(30), 256 << 20);
let observed = sandbox.run(WORKLOAD, "run", &[], &loose)?;
let burned = observed.fuel_consumed.unwrap_or(0);

// Production budget: observed cost plus 3x headroom.
let production = Limits::default().with_fuel(burned.saturating_mul(3));
let _ = sandbox.run(WORKLOAD, "run", &[], &production)?;
# const WORKLOAD: &[u8] = b"";
# Ok::<(), sandboxd::SandboxError>(())
```

See [Configuration and Tuning](Configuration-and-Tuning) for the full method.

## Branch on why a run stopped

```rust
use sandboxd::{Sandbox, Limits, SandboxError};

let sandbox = Sandbox::deny_all()?;
let limits = Limits::default();

match sandbox.run(MODULE, "run", &[], &limits) {
    Ok(out) => meter(out.fuel_consumed),
    Err(SandboxError::FuelExhausted { budget }) => log_overrun(budget),
    Err(SandboxError::Timeout { millis }) => log_slow(millis),
    Err(SandboxError::MemoryLimitExceeded { limit }) => log_oversized(limit),
    Err(SandboxError::DisallowedImport { module, name }) => audit(&module, &name),
    Err(other) => reject(&other),
}
# fn meter(_: Option<u64>) {}
# fn log_overrun(_: u64) {}
# fn log_slow(_: u64) {}
# fn log_oversized(_: usize) {}
# fn audit(_: &str, _: &str) {}
# fn reject(_: &sandboxd::SandboxError) {}
# const MODULE: &[u8] = b"";
# Ok::<(), sandboxd::SandboxError>(())
```

The full variant catalogue is in the [Error Reference](Error-Reference).

## Run many guests concurrently

`Sandbox` is `Send + Sync`; share it across threads and each `run` gets its own store and watchdog.

```rust
use std::sync::Arc;
use std::thread;
use sandboxd::{Sandbox, Limits, Value};

let sandbox = Arc::new(Sandbox::deny_all()?);
let limits = Limits::default();

let handles: Vec<_> = (0..4).map(|i| {
    let sandbox = Arc::clone(&sandbox);
    let limits = limits.clone();
    thread::spawn(move || {
        sandbox.run(WAT, "fib", &[Value::I32(i * 5)], &limits)
    })
}).collect();

for h in handles {
    let _ = h.join().unwrap();
}
# const WAT: &[u8] = b"";
# Ok::<(), sandboxd::SandboxError>(())
```

## Pre-validate a module before accepting it

If you take uploaded modules, reject the unparseable ones up front with `compile`, which validates without running:

```rust
use sandboxd::{Sandbox, SandboxError};

let sandbox = Sandbox::deny_all()?;
match sandbox.compile(uploaded_bytes) {
    Ok(_module) => accept(),
    Err(SandboxError::InvalidModule(msg)) => reject_with(&msg),
    Err(e) => reject_with(&e.to_string()),
}
# fn accept() {}
# fn reject_with(_: &str) {}
# let uploaded_bytes: &[u8] = b"";
# Ok::<(), sandboxd::SandboxError>(())
```

Note `run` recompiles internally, so this is for validation, not caching.

## The same recipes from the CLI

For quick experiments without writing Rust, the binary covers the common cases. See [CLI Usage](CLI-Usage) for the full flag set and exit codes.

```bash
# pure function
sandboxd fixtures/well_behaved.wat --invoke add --arg 2 --arg 40

# tight budget that kills a loop
sandboxd fixtures/infinite_loop.wat --fuel 1000000          # exit 2

# grant the log capability
sandboxd fixtures/logger.wat --allow-log
```

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
