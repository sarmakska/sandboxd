# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Core sandbox engine built on wasmtime with three independent, enforced limits: fuel metering for a deterministic instruction budget, epoch interruption with a watchdog thread for a wall-clock deadline, and a `ResourceLimiter` for a linear memory cap.
- Deny-by-default host ABI. No WASI and no ambient host functions; the embedder opts in to each capability. The one audited capability today is `host::log`, which copies a bounds-checked UTF-8 string out of guest memory into an auditable sink.
- Typed error surface (`SandboxError`) with a distinct variant per failure mode: fuel exhausted, timeout, memory limit exceeded, disallowed import, invalid module, export error and guest trap.
- Library API: `Sandbox`, `Limits`, `HostAbi`, `Value` and `RunOutput`.
- CLI with flags for the export name, fuel budget, wall-clock timeout, memory cap, integer arguments and the log capability grant, with a distinct process exit code per failure category.
- Five `.wat` fixtures: an infinite loop, an over-allocating module, a module with a disallowed import, a logging module and a pure well-behaved module.
- Integration test suite covering fuel exhaustion, epoch timeout, the memory cap, disallowed-import rejection at instantiation, the allowed import path, determinism of a pure module, and the error paths for missing exports, signature mismatches and invalid modules.
- Documentation: a product-page README with a Mermaid architecture diagram and a benchmarks section, plus a full wiki (Home, Architecture, Threat Model, Resource Limits, Host ABI, CLI Usage, Troubleshooting).
- Continuous integration that builds, lints with clippy and runs the test suite on push and pull request.
