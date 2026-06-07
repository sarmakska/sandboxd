# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Second audited host capability, `host::random`: a seeded, deterministic 64-bit generator behind an explicit `HostAbi::allow_random(seed)` grant and a `--seed` CLI flag. It follows the same deny-by-default recipe as `host::log` and is reproducible per seed, so a module that draws random numbers stays as replayable as a pure one. It is a splitmix64 advance with no external dependency and is explicitly not a cryptographic source.
- Per-run peak linear memory on `RunOutput.peak_memory_bytes`, reported on the CLI too, so an embedder can size the memory cap from one observed run alongside the existing fuel figure.
- Two fixtures (`random.wat`, `grow_within_cap.wat`) and four tests covering deny-by-default rejection of `host::random`, determinism of the seeded stream, divergence across seeds, and the peak-memory high-water mark.
- Core sandbox engine built on wasmtime with three independent, enforced limits: fuel metering for a deterministic instruction budget, epoch interruption with a watchdog thread for a wall-clock deadline, and a `ResourceLimiter` for a linear memory cap.
- Deny-by-default host ABI. No WASI and no ambient host functions; the embedder opts in to each capability. The one audited capability today is `host::log`, which copies a bounds-checked UTF-8 string out of guest memory into an auditable sink.
- Typed error surface (`SandboxError`) with a distinct variant per failure mode: fuel exhausted, timeout, memory limit exceeded, disallowed import, invalid module, export error and guest trap.
- Library API: `Sandbox`, `Limits`, `HostAbi`, `Value` and `RunOutput`.
- CLI with flags for the export name, fuel budget, wall-clock timeout, memory cap, integer arguments and the log capability grant, with a distinct process exit code per failure category.
- `.wat` fixtures: an infinite loop, an over-allocating module, a module with a disallowed import, a logging module, a seeded-random module, a bounded-growth module and a pure well-behaved module.
- Integration test suite covering fuel exhaustion, epoch timeout, the memory cap, disallowed-import rejection at instantiation, the allowed import path, determinism of a pure module, and the error paths for missing exports, signature mismatches and invalid modules.
- Documentation: a product-page README with a Mermaid architecture diagram and a benchmarks section, plus a full wiki (Home, Architecture, Threat Model, Resource Limits, Host ABI, CLI Usage, Troubleshooting).
- Continuous integration that builds, lints with clippy and runs the test suite on push and pull request.
- SarmaLinux brand: a README banner and shield mark under `.github/brand/`, palette-themed Mermaid diagrams, and the project footer.

### Changed

- README rewritten to open from the attacker's point of view, with an attack-fixture table, a design-decisions section naming the alternatives I rejected, real measured numbers from an Apple M3 Pro, and an explicit limitations and roadmap.
- Wiki lifted to a 30-second Home with a system diagram and navigation table, a new Roadmap and Limitations page, and palette-themed diagrams throughout.
