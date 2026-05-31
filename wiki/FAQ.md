# FAQ

Short answers to the questions I expect an adopter to ask. Where a question deserves a full treatment, the answer points to the page that has it.

## Is this production-ready?

It is version 0.1. The isolation guarantees are tested ([Testing Strategy](Testing-Strategy)) and the design is deliberately small, but the version number is honest: the API may change before 1.0, and the [roadmap](Roadmap-and-Limitations) lists capabilities not yet built. Read the [Threat Model](Threat-Model) and decide against your own risk tolerance.

## Does it use WASI?

No, and that is deliberate. There is no filesystem, network, clock or environment unless you build and audit a capability for it. A guest compiled for `wasm32-wasi` is rejected at instantiation. Build guests for `wasm32-unknown-unknown`. The reasoning is in [Design Decisions](Design-Decisions).

## How is an infinite loop stopped?

Two independent ways. Fuel deducts per instruction until the budget hits zero (`FuelExhausted`). Independently, if you give it near-infinite fuel, the watchdog bumps the engine epoch after the timeout and the guest is interrupted (`Timeout`). The same `infinite_loop.wat` fixture is tested against both. See [The Watchdog and Epoch Interruption](The-Watchdog-and-Epoch-Interruption).

## Why both fuel and a timeout? Is one not enough?

Each has a blind spot. Fuel is deterministic but does not count time spent inside a granted host call, so a guest could hold a thread while burning almost no fuel. The timeout counts wall-clock time but is not deterministic, so it cannot serve as a replayable quota. Carrying both is cheap and each covers the other. See [Design Decisions](Design-Decisions).

## Is fuel consumption really deterministic?

Yes, for a pure module. The same module on the same inputs burns exactly the same fuel every run, regardless of CPU speed. `fib(30)` is 522 fuel every time. The `pure_module_is_deterministic` test asserts it across three fresh sandboxes. That is what lets you use fuel as a quota or a billing unit.

## Can I run the same `Sandbox` from multiple threads?

Yes. `Sandbox` is `Send + Sync`. Each `run` builds its own store and its own watchdog, so concurrent runs share nothing but the immutable engine. There is a concurrency recipe in [Examples and Recipes](Examples-and-Recipes).

## How do I let a guest do output?

Grant the audited `host::log` capability with `HostAbi::deny_all().allow_log()`. The guest writes a UTF-8 string into its memory and calls `host::log` with a pointer and length; the host copies it into a sink you own, with full bounds checking. See [Host ABI](Host-ABI).

## How do I add my own capability, say a clock?

Follow the five-step recipe in [Writing a Host Capability](Writing-a-Host-Capability): add a field and `allow_*` builder, teach the allow-list, define the function validating every guest-controlled argument, never hand back a pointer, and test it denied-by-default and working-when-granted. A clock and a seeded RNG are on the roadmap because they take no guest pointers and so are easy to make safe.

## What happens when a guest hits the memory cap?

`memory.grow` returns -1 to the guest (the limiter refuses the growth). Whatever the guest does next, an `unreachable`, an out-of-bounds store, or a plain trap, the run is reported as `MemoryLimitExceeded`, because the limiter records the denial and `run` checks that before classifying the trap. See [The Sandbox Engine](The-Sandbox-Engine).

## Why is the binary 12 MB?

It statically links wasmtime and the Cranelift code generator. That is also why the first build is slow. Numbers are in [Performance and Benchmarks](Performance-and-Benchmarks).

## How fast is it?

On an Apple M3 Pro, 100 cold CLI invocations of `fib(30)` total 1.025 s, about 10.3 ms each including process spawn and module compile. In-process embedding is faster per call because spawn cost is paid once; the dominant remaining cost is compilation. Full figures in [Performance and Benchmarks](Performance-and-Benchmarks).

## Can a guest read my host memory through `host::log`?

No. The host validates the pointer and `pointer + length` against the guest's actual memory size before reading, rejects negative or overflowing values, and never hands a pointer back to the guest. A malformed pointer traps cleanly. The implementation is small enough to audit in full; see [Host ABI](Host-ABI).

## Does it protect against side channels?

No. Timing, cache and speculative-execution leakage between guests sharing a machine is out of scope. If two mutually distrusting guests share hardware, sandboxd does not stop one inferring things about the other through microarchitectural state. That is in the [Threat Model](Threat-Model) and the [limitations](Roadmap-and-Limitations).

## What if wasmtime has a sandbox escape?

Then sandboxd has one too. The isolation rests entirely on wasmtime and Cranelift being correct. Keep the dependency current and report escapes upstream. This is stated plainly in the [Threat Model](Threat-Model).

## Why only i32 arguments on the CLI?

To keep the binary's argument parsing simple. The library handles the full scalar set (`i32`, `i64`, `f32`, `f64`). For other types or structured returns, embed the library. See [CLI Usage](CLI-Usage).

## How do I report a security issue?

Email security@sarmalinux.com privately. Do not open a public issue. The disclosure process and timelines are in [SECURITY.md](https://github.com/sarmakska/sandboxd/blob/main/SECURITY.md).

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
