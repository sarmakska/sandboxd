# Configuration and Tuning

How to choose fuel, timeout and memory for a real workload, with a method rather than guesses. [Resource Limits](Resource-Limits) explains what each limit enforces; this page is about picking values. Every number below was measured on an Apple M3 Pro (macOS 26.3, Rust 1.96, release build) against the shipped fixtures.

## The one rule: measure, then add headroom

The limits are not knobs to tweak blindly. The right value for fuel and memory comes from observing a representative run. `RunOutput::fuel_consumed` exists precisely so you can do this:

```rust
let generous = Limits::new(u64::MAX / 2, Duration::from_secs(30), 256 * 1024 * 1024);
let out = sandbox.run(bytes, "run", &params, &generous)?;
println!("this workload burned {:?} fuel", out.fuel_consumed);
// now set the real limit to fuel_consumed plus headroom
```

Run your heaviest legitimate input under a deliberately loose budget, read the fuel it burned, then set the production budget to that plus margin for inputs you have not seen.

## Fuel

Fuel is the deterministic CPU bound, consumed per instruction. Same module, same input, same fuel, every time. That determinism is what makes it usable as a quota or a billing unit.

Measured fuel costs for the fixtures:

| Call | Fuel consumed |
| --- | --- |
| `add(2, 40)` | 4 |
| `fib(10)` | 182 |
| `fib(20)` | 352 |
| `fib(30)` | 522 |

`fib` grows by 170 fuel per 10 iterations, which is the per-loop-iteration cost times ten. The numbers are small integers because each WebAssembly instruction is a handful of fuel units. Real workloads land in the thousands to millions; the method is the same.

**Picking a budget.** Take the observed cost of your worst legitimate input and multiply by a safety factor (2x to 4x is reasonable depending on input variance). The default of 100,000,000 is generous; it lets meaningful work run while still stopping an infinite loop in a fraction of a second.

**Using fuel as a quota.** Because it is deterministic, you can bill or rate-limit on `fuel_consumed` and reproduce the figure exactly. Two callers running the same module on the same input owe the same amount.

## Timeout

The wall-clock deadline is a safety net, not the primary control. Prefer fuel for predictable CPU bounds, and set the timeout above the worst-case fuel-bounded run time so it fires only for guests that are genuinely stuck (spinning in a host call, descheduled, or otherwise not burning fuel).

**Picking a timeout.** Estimate the wall-clock time of your fuel budget at full burn, then set the timeout a few multiples higher. From the benchmarks, a fuel-bounded infinite loop with a 1,000,000 budget stops in about 29 ms end to end including process spawn and compile, so the in-guest compute portion is small. If your fuel budget is 100,000,000 (100x that), expect low single-digit seconds at most; a 5 to 10 second timeout leaves clear headroom.

**Precision.** The watchdog polls in 5 ms slices, so the deadline fires within one slice of the configured value. Do not expect microsecond precision. Sub-millisecond timeouts are rounded up to 1 ms. A 100 ms deadline fired at 145 to 147 ms end to end in testing, where the extra is spawn and compile, not slack.

## Memory

The cap bounds total linear memory. wasmtime grows memory in 64 KiB pages, so the effective cap is rounded down to a page boundary.

**Picking a cap.** Set `memory_bytes` to the largest working set a legitimate guest needs plus headroom. There is currently no per-run memory high-water reported (it is on the [roadmap](Roadmap-and-Limitations)), so size it from what your guest's data structures require. The default 16 MiB suits small compute kernels; raise it for guests that build large buffers.

**Tables and instances.** `table_elements` (default 10,000) and `instances` (default 1) rarely need changing. Raise `instances` only if you intentionally let a module create more than one instance. These have no `with_*` builder; set the field directly:

```rust
let mut limits = Limits::default();
limits.instances = 2;
limits.table_elements = 50_000;
```

## Putting it together

```rust
use std::time::Duration;
use sandboxd::Limits;

// A tight profile for short, untrusted compute kernels.
let strict = Limits::new(2_000_000, Duration::from_millis(250), 8 * 1024 * 1024);

// A looser profile for a known-heavier but still bounded workload.
let heavy = Limits::default()
    .with_fuel(500_000_000)
    .with_timeout(Duration::from_secs(10))
    .with_memory_bytes(128 * 1024 * 1024);
```

## A profiling pass, drawn out

```mermaid
%%{init: {'theme':'base','themeVariables':{'primaryColor':'#0d1117','primaryTextColor':'#f5f7fa','primaryBorderColor':'#38bdf8','lineColor':'#22d3ee','secondaryColor':'#0f172a','tertiaryColor':'#0d1117','fontFamily':'ui-monospace, monospace'}}}%%
flowchart LR
    A[Worst legitimate input] --> B[run under loose limits]
    B --> C[read fuel_consumed]
    C --> D[fuel = observed x safety factor]
    D --> E[timeout = est wall-clock x margin]
    E --> F[memory = working set + headroom]
    F --> G[lock in production Limits]
```

## Common mistakes

- **Setting fuel from a single small input.** Profile your worst case, not your demo input.
- **Relying on fuel alone when you grant a capability.** Host-call time is invisible to fuel. Keep the timeout meaningful whenever a capability is granted. See [The Watchdog and Epoch Interruption](The-Watchdog-and-Epoch-Interruption).
- **Setting the timeout below the fuel-bounded run time.** Then the timeout fires on legitimate heavy work instead of only on stuck guests, and you lose the deterministic fuel bound as your primary control.
- **Forgetting the page rounding on memory.** A cap of 100,000 bytes is effectively one 64 KiB page (65,536 bytes), because growth happens a page at a time.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
