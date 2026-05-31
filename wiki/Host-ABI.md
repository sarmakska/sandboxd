# Host ABI

The host ABI is the surface between the guest and your process. sandboxd's design here is the heart of the project: deny everything, then let the embedder grant capabilities one at a time, by name, with code they can audit.

## Deny by default

There is no WASI in sandboxd, and there are no ambient host functions. A freshly built `HostAbi` defines nothing:

```rust
let host = HostAbi::deny_all();   // grants nothing
let sandbox = Sandbox::new(host)?;
```

When `Sandbox::run` instantiates a module it first walks every declared import and rejects any that is not on the allow-list:

```rust
fn reject_disallowed_imports(&self, module: &Module) -> Result<()> {
    for import in module.imports() {
        let allowed = matches!(
            (import.module(), import.name(), self.host.log_allowed()),
            ("host", "log", true)
        );
        if !allowed {
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

## Adding your own capability

If you extend the host, follow the same discipline:

1. Add a field to `HostAbi` and an `allow_*` builder that turns it on.
2. Update `log_allowed`-style checks (or generalise them) so `reject_disallowed_imports` recognises the new import.
3. Define the function in `register`, validating every argument the guest controls before acting on it.
4. Never hand the guest a raw host pointer, and bounds-check every read from guest memory with `get`.
5. Audit it, and write a test that proves it is denied by default and works when granted.

The shipped `host::log` is the worked reference for all five steps.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
