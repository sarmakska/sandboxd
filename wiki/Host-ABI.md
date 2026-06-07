# Host ABI

The host ABI is the surface between the guest and your process. sandboxd's design here is the heart of the project: deny everything, then let the embedder grant capabilities one at a time, by name, with code they can audit.

## Deny by default

There is no WASI in sandboxd, and there are no ambient host functions. A freshly built `HostAbi` defines nothing:

```rust
let host = HostAbi::deny_all();   // grants nothing
let sandbox = Sandbox::new(host)?;
```

When `Sandbox::run` instantiates a module it first walks every declared import and rejects any that is not on the allow-list. The allow-list lives in one function, `import_is_allowed`, so the pre-instantiation check and the instantiation-error mapping read from the same source and cannot drift apart:

```rust
fn import_is_allowed(module: &str, name: &str, host: &HostAbi) -> bool {
    match (module, name) {
        ("host", "log") => host.log_allowed(),
        ("host", "random") => host.random_allowed(),
        _ => false,
    }
}

fn reject_disallowed_imports(&self, module: &Module) -> Result<()> {
    for import in module.imports() {
        if !import_is_allowed(import.module(), import.name(), &self.host) {
            return Err(SandboxError::DisallowedImport {
                module: import.module().to_string(),
                name: import.name().to_string(),
            });
        }
    }
    Ok(())
}
```

This check runs before the store is built, so a hostile module that imports something it should not is turned away before any of its code executes. The error names the exact import (`env::secret`, `host::log`, whatever it was), which makes the rejection actionable rather than mysterious.

The enforcement is belt and braces. Even if the import inspection were bypassed, the linker only defines the imports that were granted, so wasmtime's own instantiation would fail on the missing import; `map_instantiation_error` translates that failure back into a `DisallowedImport` too.

## The audited capability: `host::log`

The one capability shipped today lets a guest emit a UTF-8 string for observability, and nothing else. You opt in explicitly and receive a sink that captures every line:

```rust
let (host, log_sink) = HostAbi::deny_all().allow_log();
let sandbox = Sandbox::new(host)?;
// ... run a module that imports host::log ...
for line in log_sink.lock().unwrap().iter() {
    println!("guest said: {line}");
}
```

The import signature is `(param i32 i32)`: a pointer and a length into the guest's own linear memory. The guest must export its memory as `memory`. The host reads the bytes out, validates them, and appends the resulting string to the sink.

### Why it is safe

The implementation in `src/host.rs` is small enough to audit in full. The bounds-checking is the important part:

```rust
let data = memory.data(&caller);
let end = ptr.checked_add(len)
    .ok_or_else(|| Error::msg("pointer plus length overflows in host::log"))?;
let bytes = data.get(ptr..end)
    .ok_or_else(|| Error::msg("host::log pointer or length out of bounds"))?;
Ok(String::from_utf8_lossy(bytes).into_owned())
```

- `ptr` and `len` are validated as non-negative before use.
- `checked_add` rejects a pointer-plus-length that would overflow.
- The slice is taken with `get`, which returns `None` (and so an error, and so a trap) if the range is out of bounds. A malicious guest cannot read host memory.
- `from_utf8_lossy` means a guest cannot abort the host with invalid bytes; bad bytes become replacement characters.

The host never returns a pointer or handle to the guest, so there is no path from this import back into host address space. The captured lines go into a sink you own, which is exactly the audit trail you want for untrusted code.

## The second audited capability: `host::random`

The second capability hands the guest a seeded, deterministic stream of 64-bit numbers, and nothing else. You opt in with a seed:

```rust
let host = HostAbi::deny_all().allow_random(42);
let sandbox = Sandbox::new(host)?;
// ... run a module that imports host::random ...
```

The import signature is `() -> i64`: no pointer, no length, no memory access. Each call advances a splitmix64 generator held in an atomic and returns the next value:

```rust
fn next_random(state: &AtomicU64) -> u64 {
    let z = state.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    let z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
```

### Why it is safe, and what it is not for

- There is no argument the guest controls and no pointer behind the call, so there is nothing to bounds-check and no path into host memory.
- It is seeded, so the project's reproducibility guarantee holds: the same seed and the same number of calls produce the same stream every run. A module that draws random numbers stays as replayable as a pure one, which is what lets fuel keep doubling as a quota or a billing unit.
- splitmix64 passes the usual statistical tests for a non-cryptographic generator and needs no external dependency.
- It is **not** a cryptographic source. Do not use it for keys, nonces or anything where unpredictability matters. If a guest legitimately needs cryptographic randomness, that is a different, explicit capability you would design and audit separately.

## Adding your own capability

If you extend the host, follow the same discipline:

1. Add a field to `HostAbi` and an `allow_*` builder that turns it on.
2. Add the new import to `import_is_allowed` so both the pre-check and the instantiation-error mapping recognise it.
3. Define the function in `register`, validating every argument the guest controls before acting on it.
4. Never hand the guest a raw host pointer, and bounds-check every read from guest memory with `get`.
5. Audit it, and write a test that proves it is denied by default and works when granted.

The shipped `host::log` and `host::random` are the worked references: `host::log` for the case where the guest hands you memory you must validate, and `host::random` for the case where there is no guest-controlled input at all.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
