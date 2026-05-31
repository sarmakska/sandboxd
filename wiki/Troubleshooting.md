# Troubleshooting

This page lists the errors you are most likely to hit, what each one means, and how to fix it. Every error maps to a `SandboxError` variant and, on the CLI, to a distinct exit code (see [CLI Usage](CLI-Usage)).

## `fuel exhausted: the module exceeded its instruction budget`

The guest executed more instructions than its fuel budget allowed. This is expected for an infinite loop or for a genuinely heavy workload run under too small a budget.

- If the module is legitimate, raise `--fuel` (or the `fuel` field on `Limits`). Run once with a generous budget and read the reported `fuel consumed` to size it.
- If the module should not loop, the budget is doing its job; fix the guest.

## `wall-clock timeout: the module ran longer than N ms`

The guest ran past its wall-clock deadline and the epoch watchdog interrupted it.

- For a legitimate slow workload, raise `--timeout-ms` (or the `timeout` on `Limits`).
- If a module that should finish quickly is timing out instead of exhausting fuel, check whether it is blocking inside a host call; time spent there does not consume fuel but does count against the wall clock.

## `memory limit exceeded: the module requested more than N bytes`

The guest tried to grow its linear memory or tables past the cap.

- Raise `--memory-mb` (or `memory_bytes` on `Limits`) if the working set is legitimately large. Remember memory grows in 64 KiB pages, so the effective cap is rounded to a page boundary.
- If the module should not need that much, the cap is working; the module is the problem.

## `disallowed import: the module imports X::Y which is not on the allow-list`

The module declared an import that the host did not grant. This is the deny-by-default contract working as designed.

- If the import is `host::log`, grant it with `--allow-log` (CLI) or `HostAbi::deny_all().allow_log()` (library).
- If it is anything else (for example a WASI import like `wasi_snapshot_preview1::fd_write`, or `env::*`), sandboxd does not provide it by design. Recompile the guest without that dependency, or extend the host ABI yourself following the recipe in [Host ABI](Host-ABI). Do not expect WASI to be available; there is none.

A common cause is compiling a guest with a standard library that pulls in WASI. Build the guest for a freestanding target (for example `wasm32-unknown-unknown` rather than `wasm32-wasi`) so it does not import the host facilities sandboxd refuses to provide.

## `invalid module: ...`

The bytes were not valid WebAssembly or WAT, or failed validation.

- Confirm the file is a real `.wasm` or `.wat`. sandboxd parses WAT directly, so either is fine, but plain text that is not WAT will fail here.
- A truncated or corrupted `.wasm` produces this too.

## `export error: no exported function named X` or signature mismatch

The function you asked to invoke does not exist, or you passed the wrong number or types of arguments.

- Check the export name. The CLI default is `run`; pass `--invoke` to call a different export.
- The CLI passes i32 arguments only. If the function expects other types, use the library API and supply matching `Value` variants. A parameter-count or type mismatch is reported rather than panicking.

## `host::log requires the guest to export its linear memory as 'memory'`

You granted the log capability but the guest does not export its memory under the name `memory`, so the host cannot read the string.

- Ensure the module has `(memory (export "memory") ...)`. The `logger.wat` fixture is the reference.

## The first build takes a long time

This is expected. wasmtime and Cranelift are large crates and the first compile builds them from scratch. Subsequent builds reuse the cached artefacts and are fast. The CI workflow caches the cargo registry and `target` directory to keep pipeline times down.

## `cargo fmt --check` fails in CI but the code looks fine

Run `cargo fmt --all` locally and commit the result. CI enforces formatting with `cargo fmt --all --check`, so any unformatted code fails the pipeline. If `rustfmt` is missing locally, install it with `rustup component add rustfmt`.
