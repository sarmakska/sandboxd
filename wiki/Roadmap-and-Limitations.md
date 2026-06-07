# Roadmap and Limitations

What I plan to do next, and an honest account of where the boundary sits today. A project that only lists what it does is hiding half the picture.

## Limitations

These are real and current. None is a bug; each is a consequence of the scope.

- **Side channels are out of scope.** Timing, cache and speculative-execution leakage between guests on the same machine is not addressed. Two mutually distrusting guests can still infer things about each other through microarchitectural state. If that is your threat, you need process or hardware isolation on top.
- **Denial of service within the limits is not prevented.** A guest that respects its fuel, time and memory budgets can still spend all of them on every call. Sizing budgets and rate-limiting callers is the embedder's job.
- **Isolation is only as sound as wasmtime.** The whole boundary rests on wasmtime and Cranelift being correct. A sandbox escape there is an escape here. Track the dependency and report upstream.
- **No WASI, and that is deliberate.** There is no filesystem, network, clock or environment unless you build and audit a capability for it. A guest compiled against `wasm32-wasi` will be rejected at instantiation. Build guests for `wasm32-unknown-unknown` or grant the specific capability you need.
- **The CLI takes i32 arguments only.** The library handles the full scalar set (`i32`, `i64`, `f32`, `f64`); the binary keeps its argument parsing simple. For other types or structured returns, embed the library.
- **Host-call time is bounded by the watchdog, not by fuel.** Time spent inside a granted host function does not consume fuel. The wall-clock deadline still bounds it, but do not rely on fuel alone to cap work that happens on the host side of an import.

## Shipped

These were on the roadmap and are now in the crate.

- **A seeded-RNG capability (`host::random`)**, behind an explicit `allow_random(seed)` grant and following the `host::log` recipe in [Host ABI](Host-ABI): denied by default, no guest-controlled input to validate, no pointer handed back, with denied-by-default and determinism tests. Reproducible per seed and explicitly non-cryptographic.
- **Memory high-water returned alongside fuel** on `RunOutput.peak_memory_bytes` (and on the CLI), so an embedder can size both limits from a single observed run rather than guessing the memory cap.

## Roadmap

Ordered roughly by how likely I am to ship it.

1. **A monotonic-clock capability**, behind an explicit grant and following the same `host::log` recipe: validate everything the guest controls, never hand back a pointer, write a denied-by-default test. A common need that is genuinely easy to make safe, and the natural companion to the seeded RNG.
2. **Optional `.wasm` precompilation and an artefact cache** for embedders that run the same module repeatedly, to skip the per-run compile cost.
3. **A streaming log sink option** so an embedder can consume guest log lines as they arrive rather than after the run, useful for long-running grants.

## Things I will not add

Saying no keeps the project small enough to trust.

- No plugin manager, package format or registry. The contract is "run these bytes under these limits"; discovery and distribution are someone else's layer.
- No network capability of any kind. If a guest needs the network, that is a different trust decision and a different tool.
- No general WASI shim. Adding all of WASI and clawing it back is the deny-list posture this project exists to avoid.
- No async or multi-threaded guest execution in the core. A run is one call on one thread with one budget; that simplicity is load-bearing for the isolation argument.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
