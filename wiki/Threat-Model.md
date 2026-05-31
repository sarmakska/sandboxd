# Threat Model

This page states plainly what sandboxd defends against, what it does not, and the guarantee behind each limit. Read it before you put untrusted code in front of it.

## The adversary

The guest is assumed to be fully hostile. It may:

- spin forever or recurse without bound,
- try to allocate unbounded memory,
- import host functions it is not entitled to,
- pass malformed pointers and lengths to any host function it is given,
- contain a deliberately crafted module designed to trip wasmtime.

The embedder (the code calling sandboxd) is trusted. The host machine and the wasmtime build are trusted.

## What sandboxd guarantees

### 1. Bounded CPU

A run cannot execute more WebAssembly instructions than its fuel budget. Fuel is consumed deterministically per instruction, so this bound is independent of how fast or slow the machine is. A module that exhausts its fuel is stopped with `SandboxError::FuelExhausted`. This is verified by the `fuel_exhaustion_terminates` test, which runs an infinite loop with a long timeout and asserts that fuel, not time, is what stops it.

### 2. Bounded wall-clock time

A run cannot occupy a thread past its configured deadline. The epoch watchdog interrupts the guest at the next safe point after the timeout elapses, producing `SandboxError::Timeout`. This catches code that does not burn fuel predictably, for example a tight loop that the watchdog stops even with an effectively unlimited fuel budget. Verified by `epoch_timeout_terminates`.

### 3. Bounded memory

A run cannot grow its linear memory or tables beyond the configured caps. The `ResourceLimiter` refuses the growth request at the cap; the guest sees a failed `memory.grow` and, however it reacts, the run is reported as `SandboxError::MemoryLimitExceeded`. Verified by `memory_cap_enforced`.

### 4. No ambient authority

A freshly built `HostAbi` grants nothing. There is no WASI, no clock, no filesystem, no network, no environment. A module that imports anything not explicitly granted is rejected at instantiation, before any of its code runs, with `SandboxError::DisallowedImport` naming the offending import. Verified by `disallowed_import_rejected` and `log_import_denied_by_default`.

### 5. A safe host boundary for granted capabilities

The one capability shipped today, `host::log`, reads a string out of guest memory with full bounds checking. A malformed pointer or length traps cleanly rather than reading host memory, and invalid UTF-8 is replaced rather than allowed to abort the host. The host never hands the guest a pointer back, so there is no path from the import into host address space.

### 6. Run isolation

Each run uses a fresh store. Fuel, the epoch deadline, the memory limiter, linear memory and globals are all per-store, so one run cannot observe or influence another.

## What sandboxd does not defend against

These are explicitly out of scope. Stating them is part of being honest about the boundary.

- **Side channels.** Timing, cache and speculative-execution channels are not addressed. If two mutually distrusting guests share a machine, sandboxd does not stop one from learning about the other through microarchitectural state.
- **Denial of service within the limits.** A guest that stays under its fuel, time and memory budgets can still consume the full budget on every call. Provisioning and rate limiting are the embedder's responsibility.
- **Bugs in wasmtime or Cranelift.** sandboxd's isolation rests on wasmtime's correctness. A sandbox escape in wasmtime is a sandbox escape in sandboxd. Keep the dependency current and report such issues upstream.
- **Host code the embedder writes.** If you grant a capability whose implementation is unsafe, sandboxd cannot save you. The shipped `host::log` is audited; anything you add is yours to audit.
- **Resource accounting outside the WebAssembly instruction stream.** Time spent inside a host call is bounded by the wall-clock watchdog but does not consume fuel, so do not rely on fuel alone to bound work that happens on the host side of an import.

## Reporting

If you find a way for a guest to escape any of the guarantees in the first section, please follow the disclosure process in [SECURITY.md](https://github.com/sarmakska/sandboxd/blob/main/SECURITY.md): email security@sarmalinux.com and expect an acknowledgement within 7 days.
