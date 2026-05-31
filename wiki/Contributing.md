# Contributing

How to build, test and extend sandboxd, and what I will and will not take. The project is small on purpose, so the most valuable contributions keep it that way.

## Build and test locally

```bash
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
cargo build
cargo test
```

The first build compiles wasmtime and Cranelift and is slow; later builds reuse the cached artefacts. The test suite is fast once built (0.13 s for the eleven integration tests on an M3 Pro). MSRV is Rust 1.80, edition 2021.

## What CI will check

The workflow in `.github/workflows/ci.yml` runs on push and pull request to `main` with `RUSTFLAGS: "-D warnings"`. Match it locally before you push:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --verbose
cargo test --verbose
```

If `cargo fmt --check` fails, run `cargo fmt --all` and commit the result. If `rustfmt` or `clippy` are missing, add them with `rustup component add rustfmt clippy`. CI caches the registry and `target` on the `Cargo.lock` hash; do not commit `target`.

## The code map

| File | What lives there |
| --- | --- |
| `src/lib.rs` | the public re-exports, the quick-start doc-test |
| `src/sandbox.rs` | `Sandbox`, `Value`, `RunOutput`, the run flow, the watchdog, trap classification |
| `src/limits.rs` | `Limits`, `StoreState`, the `ResourceLimiter` impl |
| `src/host.rs` | `HostAbi`, the allow-list, `host::log` and its bounds-checked memory read |
| `src/error.rs` | `SandboxError` and the `Result` alias |
| `src/main.rs` | the CLI, flag parsing, exit-code mapping |
| `tests/sandbox.rs` | the eleven integration tests |
| `fixtures/*.wat` | one fixture per behaviour, used by the tests |

[The Sandbox Engine](The-Sandbox-Engine) walks `Sandbox::run` in detail if you are changing the core.

## Adding a host capability

This is the most likely substantial contribution. Follow the five-step recipe in [Writing a Host Capability](Writing-a-Host-Capability) exactly: a field and `allow_*` builder on `HostAbi`, the allow-list update in both `reject_disallowed_imports` and `map_instantiation_error`, the definition in `register` validating every guest-controlled argument, no raw pointer ever handed back, and two tests proving denied-by-default and works-when-granted. A capability that takes guest-controlled pointers also wants a hostile fixture, the way `memory_bomb.wat` backs the memory cap.

## Adding a test or a fixture

The fixtures are the executable form of the threat model. If you add a behaviour, add a `.wat` fixture for it and an integration test that drives it through the public API and asserts the exact `SandboxError` variant. Keep fixtures small and commented; each existing one explains in its header what it attacks and how it dies. See [Testing Strategy](Testing-Strategy) for the gaps I would welcome help on (fuzzing, property tests).

## Commit and PR conventions

- Small, logically ordered commits with specific messages (`fix off-by-one in ...`, not `update code`). Use conventional prefixes: `feat`, `fix`, `test`, `docs`, `chore`, `ci`.
- Run the CI checks above locally first.
- Explain the change and, for anything touching the host boundary, the security reasoning. A capability addition without a denied-by-default test will not be merged.

## What I will not take

The scope is "run these bytes under these limits and tell me what happened". Contributions that grow it past that are out, however well written:

- a plugin manager, package format or registry,
- a network capability of any kind,
- a general WASI shim,
- async or multi-threaded guest execution in the core.

The reasoning is in the non-goals on [Roadmap and Limitations](Roadmap-and-Limitations). Saying no is what keeps the project small enough to trust.

## Reporting security issues

Do not open a public issue for a vulnerability. Email security@sarmalinux.com privately and follow the process in [SECURITY.md](https://github.com/sarmakska/sandboxd/blob/main/SECURITY.md). I acknowledge within seven days.

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
