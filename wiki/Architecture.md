# Architecture

sandboxd is a thin, opinionated layer over [wasmtime](https://wasmtime.dev/). wasmtime does the heavy lifting of compiling and executing WebAssembly; sandboxd's job is to wire up the enforcement so that running untrusted code is safe by construction, and to present a small typed API.

## Module layout

| File | Responsibility |
| --- | --- |
| `src/lib.rs` | The public surface. Re-exports `Sandbox`, `Limits`, `HostAbi`, `Value`, `RunOutput`, `SandboxError`. |
| `src/sandbox.rs` | The engine. Compiles modules, applies limits to a fresh store, instantiates and calls the export, classifies traps. |
| `src/limits.rs` | `Limits` plus the `StoreState` that implements wasmtime's `ResourceLimiter`. |
| `src/host.rs` | `HostAbi`: the deny-by-default allow-list and the audited `host::log` import. |
| `src/error.rs` | `SandboxError`: a typed variant per failure mode. |
| `src/main.rs` | The CLI. Parses flags, builds the host and limits, runs, maps errors to exit codes. |

## The engine

A `Sandbox` builds a wasmtime `Engine` once with three configuration choices that the rest of the design depends on:

```rust
let mut config = Config::new();
config.consume_fuel(true);          // enable the deterministic instruction budget
config.epoch_interruption(true);    // enable cooperative wall-clock interruption
config.cranelift_opt_level(wasmtime::OptLevel::Speed);
let engine = Engine::new(&config)?;
```

The engine is reference counted internally and safe to share, so you create one per process and reuse it across runs. Each call to `Sandbox::run` builds a fresh `Store`, which means runs share no linear memory, no globals and no fuel; one run cannot influence the next.

## The instantiate-and-limit flow

```mermaid
%%{init: {'theme':'base','themeVariables':{'primaryColor':'#0d1117','primaryTextColor':'#f5f7fa','primaryBorderColor':'#38bdf8','lineColor':'#22d3ee','secondaryColor':'#0f172a','tertiaryColor':'#0d1117','fontFamily':'ui-monospace, monospace'}}}%%
flowchart TD
    A[Untrusted bytes: .wasm or .wat] --> B[Module::new compiles and validates]
    B -->|parse or validate error| E1[SandboxError::InvalidModule]
    B --> C[reject_disallowed_imports: walk module.imports]
    C -->|import not granted| E2[SandboxError::DisallowedImport]
    C -->|all imports granted| D[Store::new with StoreState]
    D --> D1[store.set_fuel: instruction budget]
    D --> D2[store.set_epoch_deadline 1]
    D --> D3[store.limiter -> ResourceLimiter for memory]
    D1 --> F[Linker defines only granted imports]
    D2 --> F
    D3 --> F
    F --> G[Watchdog::arm: thread bumps epoch at deadline]
    G --> H[linker.instantiate]
    H -->|missing import| E2
    H --> I[instance.get_func + signature check]
    I -->|no such export or bad arity| E3[SandboxError::Export]
    I --> J[function.call]
    J --> K[Watchdog::disarm + read fuel consumed]
    K -->|Ok| R[RunOutput: values + fuel_consumed]
    K -->|growth was denied| E4[SandboxError::MemoryLimitExceeded]
    K -->|Trap::OutOfFuel| E5[SandboxError::FuelExhausted]
    K -->|Trap::Interrupt| E6[SandboxError::Timeout]
    K -->|other trap| E7[SandboxError::Trap]
```

The ordering matters. We reject disallowed imports before building the store, so a hostile module that imports something it should not is turned away before any of its code can run. We arm the watchdog immediately before the call and disarm it immediately after, so the wall-clock window covers exactly the guest's execution.

## How a trap is classified

When `function.call` returns an error, sandboxd has to decide why. The logic lives in `Sandbox::run` and `classify_trap`:

1. First it checks `store.data().growth_was_denied()`. The `ResourceLimiter` sets this flag the moment a memory or table growth is refused at the cap. This is checked first because a guest that hits the cap may react in several ways (an `unreachable`, an out-of-bounds store, or simply trapping), and they should all be reported as a memory limit breach.
2. Otherwise it downcasts the error to a wasmtime `Trap` and matches the code: `OutOfFuel` becomes `FuelExhausted`, `Interrupt` becomes `Timeout`, and a memory or table out-of-bounds becomes `MemoryLimitExceeded`.
3. Anything else is a genuine guest fault and becomes `SandboxError::Trap` carrying the backtrace.

## The watchdog

wasmtime's epoch interruption is cooperative: the guest checks an epoch counter at loop back-edges and function entries, and traps if the store's deadline has passed. sandboxd sets the store deadline to one epoch tick and then runs a `Watchdog` thread that sleeps in short slices until the wall-clock timeout elapses, at which point it calls `engine.increment_epoch()`. That bump trips the guest at its next check. The watchdog polls a shared atomic flag so that when a run finishes early it exits promptly rather than always sleeping the full timeout.

This thread-based approach is what lets sandboxd stop a guest that is spinning in pure computation, where there is no host call to piggyback on.

## Why a fresh store per run

State isolation is a correctness property, not just hygiene. Fuel is stored on the store; so is the epoch deadline and the resource limiter. By building a new store each run we guarantee that the budget is exactly what the caller asked for and that no residue from a previous run can change a later one. The engine, which holds the expensive compiled artefacts and the Cranelift backend, is the part that is reused.

## Design decisions and the roads not taken

These are the choices that shaped the code, with the alternatives I weighed and rejected.

**Two fences for compute, not one.** Fuel (`Config::consume_fuel`) is deterministic and replayable but blind to wall-clock time. Epoch interruption (`Config::epoch_interruption`) bounds time but is not deterministic. I kept both rather than picking one. The alternative, fuel only, falls down the moment you grant a host capability: time spent inside a host call does not consume fuel, so a guest could hold a thread indefinitely while burning almost nothing. The alternative, time only, throws away the replayable CPU bound that makes fuel useful as a quota. Carrying both is cheap and each covers the other's blind spot.

**A per-run watchdog thread, not a global epoch ticker.** wasmtime's epoch counter does not advance on its own. The common pattern is one long-lived thread that calls `increment_epoch` on a fixed cadence. `Watchdog::arm` instead spawns a thread per run that sleeps until that run's exact deadline, bumps once, and exits, polling a shared `AtomicBool` so it stops early when the call returns first. A global ticker is less code but gives coarse shared timing and a thread that never dies; the per-run watchdog gives each call its own precise deadline and no idle thread between runs. The price is one thread spawn per run, negligible next to compiling and running a module.

**Inspect imports ourselves before instantiation, not rely on the linker alone.** `reject_disallowed_imports` walks `module.imports()` and rejects anything off the allow-list before the store is built. wasmtime would also reject an undefined import at `Linker::instantiate`, so this looks redundant. It is deliberate belt-and-braces: doing it first means the rejection happens before any store or guest setup, and lets us name the exact offending import in the error rather than parsing wasmtime's message. `map_instantiation_error` still translates a late instantiation failure back into `DisallowedImport`, so both layers agree.

**A typed error enum, not a string or an `anyhow` blob.** Every stop reason an embedder might act on is its own `SandboxError` variant (see [Threat Model](Threat-Model) and `src/error.rs`). The cost is a little more matching in `classify_trap`; the benefit is that callers branch on the reason without scraping strings, and the CLI maps each variant to its own exit code.

## Failure modes worth knowing

- **A guest that exports no memory but is granted `host::log`.** `host::log` requires the guest to export its linear memory as `memory`. Without it the host call traps cleanly with a clear message rather than reading arbitrary bytes. See [Host ABI](Host-ABI).
- **A guest that reacts to a denied `memory.grow` in an unusual way.** Because `growth_was_denied()` is checked before trap classification, any reaction to a refused growth (an `unreachable`, an out-of-bounds store, a plain trap) is reported as `MemoryLimitExceeded`, not as a generic trap.
- **A timeout that never fires because the run finishes first.** The watchdog polls its atomic and exits without bumping the epoch, so a fast run pays only the spawn cost.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
