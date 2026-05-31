# API Reference

This page is the complete public surface of the `sandboxd` library crate, type by type and method by method, with the exact signatures from the source. Everything here is re-exported from `src/lib.rs`; nothing else is public. If a symbol is not on this page, it is private and you should not depend on it.

```rust
pub use error::{Result, SandboxError};
pub use host::{HostAbi, LogSink};
pub use limits::Limits;
pub use sandbox::{RunOutput, Sandbox, Value};
```

That re-export block in `src/lib.rs` is the contract. Six types, one alias, no traits to implement, no macros.

## `Sandbox`

Defined in `src/sandbox.rs`. A reusable sandbox bound to a single engine configuration. The engine is reference counted internally and cheap to clone, so create one per process and call `run` as many times as you like.

```rust
pub struct Sandbox { /* private */ }

impl Sandbox {
    pub fn new(host: HostAbi) -> Result<Self>;
    pub fn deny_all() -> Result<Self>;
    pub fn engine(&self) -> &wasmtime::Engine;
    pub fn compile(&self, bytes: &[u8]) -> Result<wasmtime::Module>;
    pub fn run(
        &self,
        bytes: &[u8],
        func: &str,
        params: &[Value],
        limits: &Limits,
    ) -> Result<RunOutput>;
}
```

| Method | What it does | Errors with |
| --- | --- | --- |
| `new(host)` | Build a sandbox with the given host ABI. Configures the engine for fuel and epoch interruption. | `SandboxError::Host` if the engine cannot be built |
| `deny_all()` | Shorthand for `Sandbox::new(HostAbi::deny_all())`. The safe baseline. | as `new` |
| `engine()` | Borrow the underlying `wasmtime::Engine`, for example to validate fixtures in a test. | never |
| `compile(bytes)` | Compile `.wasm` or `.wat` bytes to a `wasmtime::Module` without running it. | `SandboxError::InvalidModule` |
| `run(bytes, func, params, limits)` | Compile, check imports, apply limits, instantiate, call the export, classify the outcome. | any `SandboxError` variant |

`run` is the one method you call in normal use. `compile` and `engine` exist for embedders who want to inspect or reuse a module; note that `run` recompiles internally, so calling `compile` first does not cache anything for `run` (see the roadmap note on precompilation in [Roadmap and Limitations](Roadmap-and-Limitations)).

## `Limits`

Defined in `src/limits.rs`. The resource budget applied to a single run.

```rust
pub struct Limits {
    pub fuel: u64,
    pub timeout: std::time::Duration,
    pub memory_bytes: usize,
    pub table_elements: usize,
    pub instances: usize,
}

impl Default for Limits { /* ... */ }

impl Limits {
    pub fn new(fuel: u64, timeout: Duration, memory_bytes: usize) -> Self;
    pub fn with_fuel(self, fuel: u64) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
    pub fn with_memory_bytes(self, memory_bytes: usize) -> Self;
}
```

The five fields are public, so you can read and write them directly, but the builders read better in a chain:

```rust
let limits = Limits::default()
    .with_fuel(5_000_000)
    .with_timeout(Duration::from_millis(250))
    .with_memory_bytes(8 * 1024 * 1024);
```

Defaults: `fuel` 100,000,000, `timeout` 1 second, `memory_bytes` 16 MiB, `table_elements` 10,000, `instances` 1. The `table_elements` and `instances` caps have no dedicated builder; set the field directly if you need to change them. See [Resource Limits](Resource-Limits) for what each one bounds and [Configuration and Tuning](Configuration-and-Tuning) for how to pick values.

## `HostAbi`

Defined in `src/host.rs`. Describes which host capabilities a sandboxed module may import. Deny by default, additive only.

```rust
#[derive(Clone, Default)]
pub struct HostAbi { /* private */ }

impl HostAbi {
    pub fn deny_all() -> Self;
    pub fn allow_log(self) -> (Self, LogSink);
    pub fn log_allowed(&self) -> bool;
}
```

| Method | What it does |
| --- | --- |
| `deny_all()` | An ABI that grants nothing. Equal to `HostAbi::default()`. |
| `allow_log()` | Consume the ABI and return a new one that permits `host::log`, plus the `LogSink` that will collect what the guest emits. |
| `log_allowed()` | Whether the log capability has been granted. Used internally by the import allow-list check; also handy in your own assertions. |

`allow_log` takes `self` by value and returns the modified ABI, so it composes:

```rust
let (host, sink) = HostAbi::deny_all().allow_log();
```

To add your own capability you extend this type. The full recipe is in [Writing a Host Capability](Writing-a-Host-Capability).

## `LogSink`

```rust
pub type LogSink = std::sync::Arc<std::sync::Mutex<Vec<String>>>;
```

A shared, thread-safe buffer of the lines the guest emitted through `host::log`. You receive it from `allow_log`, and you read it after the run:

```rust
for line in sink.lock().unwrap().iter() {
    println!("guest said: {line}");
}
```

The host appends to it during the run; you read it after. It is an `Arc<Mutex<...>>` so it can be shared with the running store and still owned by you.

## `Value`

Defined in `src/sandbox.rs`. The scalar types the ABI accepts as arguments and returns as results.

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}
```

Reference types and `v128` are deliberately excluded to keep the boundary small and auditable. If you pass a `Value` whose type does not match the function's declared parameter, `run` returns `SandboxError::Export` rather than panicking; the check lives in `Sandbox::check_signature`.

## `RunOutput`

Defined in `src/sandbox.rs`. The outcome of a successful run.

```rust
#[derive(Debug, Clone)]
pub struct RunOutput {
    pub values: Vec<Value>,
    pub fuel_consumed: Option<u64>,
}
```

`values` is whatever the export returned, in order (empty for a function with no results). `fuel_consumed` is `Some` when the engine reports fuel usage, which it does whenever fuel metering is on, so in practice it is always `Some` here. It is computed as `budget - remaining`, saturating, so it never underflows.

## `SandboxError` and `Result`

Defined in `src/error.rs`. The typed reason a run did not complete normally. Each variant is documented in full on the [Error Reference](Error-Reference) page; the summary:

```rust
pub enum SandboxError {
    FuelExhausted { budget: u64 },
    Timeout { millis: u64 },
    MemoryLimitExceeded { limit: usize },
    DisallowedImport { module: String, name: String },
    InvalidModule(String),
    Export(String),
    Trap(String),
    Host(String),
}

pub type Result<T> = std::result::Result<T, SandboxError>;
```

It derives `Debug` and implements `std::error::Error` and `Display` through `thiserror`, so it slots into `?` and into `anyhow` or `eyre` if your application uses one.

## Thread-safety summary

| Type | `Send` | `Sync` | Notes |
| --- | --- | --- | --- |
| `Sandbox` | yes | yes | the engine is internally reference counted; share it across threads |
| `Limits` | yes | yes | plain data, `Clone` |
| `HostAbi` | yes | yes | `Clone`; cloning the log variant shares the same sink |
| `LogSink` | yes | yes | `Arc<Mutex<...>>` |
| `Value`, `RunOutput`, `SandboxError` | yes | yes | plain data |

A single `Sandbox` can serve concurrent `run` calls from many threads, because each call builds its own `Store` and its own watchdog. Nothing is shared between concurrent runs except the immutable engine.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
