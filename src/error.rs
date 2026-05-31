//! Typed error surface for the sandbox.
//!
//! Every failure mode an embedder cares about is represented as a distinct
//! variant so that callers can branch on the reason a module was stopped
//! rather than scraping strings out of a generic error.

use thiserror::Error;

/// The reason a sandboxed execution did not complete normally.
///
/// These variants map one-to-one onto the guarantees in the threat model:
/// a module is either stopped by a resource limit, rejected before it ever
/// runs, or it traps or returns through the normal WebAssembly path.
#[derive(Debug, Error)]
pub enum SandboxError {
    /// The module exhausted its instruction (fuel) budget. This is the hard
    /// CPU bound: it is deterministic and independent of wall-clock time.
    #[error("fuel exhausted: the module exceeded its instruction budget of {budget} units")]
    FuelExhausted {
        /// The fuel budget that was configured for the run.
        budget: u64,
    },

    /// The module ran past its wall-clock deadline and was interrupted by the
    /// epoch timer. This catches code that blocks or spins in ways that do not
    /// burn fuel predictably, for example tight host-call loops.
    #[error("wall-clock timeout: the module ran longer than {millis} ms")]
    Timeout {
        /// The configured deadline in milliseconds.
        millis: u64,
    },

    /// The module tried to grow its linear memory or tables beyond the
    /// configured cap. The growth request is denied and the module traps.
    #[error(
        "memory limit exceeded: the module requested more than {limit} bytes of linear memory"
    )]
    MemoryLimitExceeded {
        /// The configured memory cap in bytes.
        limit: usize,
    },

    /// The module imports something that is not on the host allow-list. This
    /// is detected at instantiation time, before any guest code executes.
    #[error(
        "disallowed import: the module imports `{module}::{name}` which is not on the allow-list"
    )]
    DisallowedImport {
        /// The import module namespace, for example `host`.
        module: String,
        /// The import field name, for example `log`.
        name: String,
    },

    /// The bytes handed to the sandbox were not a valid WebAssembly module or
    /// WAT source. Compilation failed before instantiation.
    #[error("invalid module: {0}")]
    InvalidModule(String),

    /// The named export was not present, or had a signature the embedder did
    /// not expect.
    #[error("export error: {0}")]
    Export(String),

    /// The guest trapped during execution for a reason that is not one of the
    /// resource limits above, for example an `unreachable` instruction, an
    /// out-of-bounds memory access, or an integer divide by zero.
    #[error("guest trap: {0}")]
    Trap(String),

    /// A host-side configuration or engine error that is not the guest's
    /// fault, for example a failure to build the engine.
    #[error("host error: {0}")]
    Host(String),
}

/// Convenience alias for results produced by the sandbox.
pub type Result<T> = std::result::Result<T, SandboxError>;
