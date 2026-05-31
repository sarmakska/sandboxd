# Resource Limits

sandboxd enforces three independent limits. Each closes a different escape route, and they are deliberately not collapsed into one: a guest that evades one is still caught by the others.

```rust
pub struct Limits {
    pub fuel: u64,            // instruction budget
    pub timeout: Duration,    // wall-clock deadline
    pub memory_bytes: usize,  // linear memory cap
    pub table_elements: usize,
    pub instances: usize,
}
```

The defaults are tight on purpose, so that an embedder who forgets to configure them still runs untrusted code under a cap:

| Field | Default | Meaning |
| --- | --- | --- |
| `fuel` | 100,000,000 | instructions |
| `timeout` | 1 second | wall-clock |
| `memory_bytes` | 16 MiB | linear memory |
| `table_elements` | 10,000 | table entries |
| `instances` | 1 | concurrent instances |

Construct them with `Limits::new(fuel, timeout, memory_bytes)` for the three primary bounds, or start from `Limits::default()` and adjust with the `with_fuel`, `with_timeout` and `with_memory_bytes` builders.

## Fuel: the deterministic CPU budget

Fuel is wasmtime's per-instruction counter. With `Config::consume_fuel(true)` set on the engine, every WebAssembly instruction the guest executes deducts from the store's fuel. When it reaches zero the guest traps with `Trap::OutOfFuel`, which sandboxd reports as `SandboxError::FuelExhausted`.

```rust
store.set_fuel(limits.fuel)?;
```

The defining property is determinism. The same module run on the same inputs consumes exactly the same fuel every time, regardless of CPU speed or load. The `pure_module_is_deterministic` test asserts this directly: `fib(20)` returns `6765` and burns identical fuel across three fresh sandboxes. That makes fuel a replayable, auditable bound, which is exactly what you want for billing, quotas or reproducible limits.

After every run sandboxd reports the fuel consumed:

```rust
let fuel_consumed = store.get_fuel().ok()
    .map(|remaining| limits.fuel.saturating_sub(remaining));
```

### Tuning fuel

Pick a budget by measuring. Run a representative workload with a generous budget, read `fuel_consumed`, and set the limit with headroom. As a rough guide from the fixtures: `add(2, 40)` costs 4 fuel, `fib(30)` costs 522 fuel. Real workloads are larger, but the point stands that fuel costs are small integers per instruction and easy to measure.

## Epoch interruption: the wall-clock deadline

Fuel bounds instructions, not time. A guest that blocks, or that the embedder lets call into slow host functions, may burn little fuel while still occupying a thread. The wall-clock timeout closes that gap.

sandboxd enables `Config::epoch_interruption(true)` and sets the store deadline to one epoch tick:

```rust
store.set_epoch_deadline(1);
```

Epoch checks are cooperative: the compiled guest tests an epoch counter at loop back-edges and function entries. To make the deadline fire, a watchdog thread sleeps until the timeout elapses and then bumps the counter:

```rust
let watchdog = Watchdog::arm(self.engine.clone(), limits.timeout);
let call_result = function.call(&mut store, &call_params, &mut results);
watchdog.disarm();
```

When the deadline passes, `engine.increment_epoch()` trips the next check and the guest traps with `Trap::Interrupt`, reported as `SandboxError::Timeout`. The watchdog polls a shared atomic so that a run finishing early stops the thread promptly rather than sleeping the whole window.

### Tuning the timeout

The timeout is a safety net, not a primary control; prefer fuel for predictable CPU bounds. Set the timeout comfortably above the worst-case fuel-bounded run time so that it only fires for genuinely stuck guests. The `epoch_timeout_terminates` test uses a 100 ms timeout with `u64::MAX` fuel and confirms the guest stops near the deadline, not seconds later.

## The memory cap

Linear memory growth is gated by wasmtime's `ResourceLimiter`, implemented by `StoreState` in `src/limits.rs`. The cap is built from `Limits` via a `StoreLimitsBuilder`:

```rust
StoreLimitsBuilder::new()
    .memory_size(self.memory_bytes)
    .table_elements(self.table_elements)
    .instances(self.instances)
    .trap_on_grow_failure(false)
    .build()
```

`trap_on_grow_failure(false)` means a growth request over the cap returns a failed `memory.grow` (a -1 to the guest) rather than aborting. sandboxd records the refusal:

```rust
fn memory_growing(&mut self, current, desired, maximum) -> wasmtime::Result<bool> {
    let allowed = self.limits.memory_growing(current, desired, maximum)?;
    if !allowed { self.growth_denied = true; }
    Ok(allowed)
}
```

After the call, if `growth_was_denied()` is set, the run is reported as `SandboxError::MemoryLimitExceeded` regardless of how the guest reacted to the failed growth. The `memory_cap_enforced` test drives the `memory_bomb.wat` fixture against a 4 MiB cap and confirms the breach is caught.

### Tuning memory

Set `memory_bytes` to the largest working set a legitimate guest needs, plus headroom. Remember that WebAssembly memory grows in 64 KiB pages, so the cap is effectively rounded down to a page boundary. The table and instance caps are usually fine at their defaults; raise `instances` only if you intentionally let a module create more than one instance.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
