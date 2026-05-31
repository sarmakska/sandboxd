# sandboxd

A WebAssembly sandbox for running untrusted code with CPU, wall-clock and memory limits and a deny-by-default host ABI.

This wiki is the reference for adopters. It explains how sandboxd enforces its limits, what the host surface looks like, exactly what the security boundary covers, and how to drive the CLI and the library.

## The idea in one paragraph

You hand sandboxd some bytes that might be hostile. sandboxd compiles them with wasmtime, checks that every import is on an allow-list you control, then runs an exported function inside a fresh store with a fuel budget, a wall-clock deadline and a memory cap. If the guest behaves it returns a value and tells you how much fuel it burned. If it misbehaves it is stopped, and you get a typed error that tells you precisely why.

## Pages

- [Architecture](Architecture): the module layout, the engine, and the instantiate-and-limit flow with a diagram.
- [Threat Model](Threat-Model): what is in scope, what is out of scope, and the guarantees each limit provides.
- [Resource Limits](Resource-Limits): fuel, epoch interruption and the memory cap, how each is enforced and tuned.
- [Host ABI](Host-ABI): the deny-by-default design and the audited `host::log` capability.
- [CLI Usage](CLI-Usage): every flag, the exit codes, and worked examples against the fixtures.
- [Troubleshooting](Troubleshooting): the errors you will hit and what they mean.

## Quick orientation

The public library surface is small on purpose:

```rust
use std::time::Duration;
use sandboxd::{Sandbox, Limits, Value};

let sandbox = Sandbox::deny_all()?;
let limits = Limits::new(1_000_000, Duration::from_millis(500), 1 << 20);
let out = sandbox.run(wat_bytes, "add", &[Value::I32(2), Value::I32(40)], &limits)?;
assert_eq!(out.values, vec![Value::I32(42)]);
# Ok::<(), sandboxd::SandboxError>(())
```

- `Sandbox` owns a configured wasmtime engine and runs modules.
- `Limits` carries the fuel budget, the timeout and the memory and table caps.
- `HostAbi` decides which imports the guest is allowed to use.
- `SandboxError` is the typed reason a run did not complete normally.

## Design principles

1. **Deny by default.** Nothing is granted to the guest unless the embedder asks for it by name. There is no WASI.
2. **Independent limits.** Fuel, time and memory are three separate fences. A guest that slips past one is still caught by the others.
3. **Typed failures.** Every stop reason is a distinct error variant, so callers branch on the reason rather than parsing strings.
4. **Determinism where it matters.** A pure module burns the same fuel on every run, which makes the CPU bound replayable.
5. **A small, auditable surface.** The host boundary is a single file you can read in a few minutes.
