//! sandboxd: a WebAssembly sandbox for running untrusted code under hard
//! limits with a deny-by-default host ABI.
//!
//! # What this gives you
//!
//! - A deterministic CPU budget via fuel metering.
//! - A wall-clock timeout via epoch interruption and a watchdog thread.
//! - A linear memory cap enforced by a [`wasmtime::ResourceLimiter`].
//! - No WASI and no ambient host functions. The guest can only call host
//!   capabilities the embedder explicitly grants, today `host::log` and a
//!   seeded, deterministic `host::random`.
//! - A per-run report of fuel consumed and the peak linear memory reached, so
//!   an embedder can size limits from one observed run.
//!
//! # Quick start
//!
//! ```no_run
//! use std::time::Duration;
//! use sandboxd::{Sandbox, Limits, Value};
//!
//! // A pure module that adds two i32 values.
//! let wat = r#"
//!   (module
//!     (func (export "add") (param i32 i32) (result i32)
//!       local.get 0
//!       local.get 1
//!       i32.add))
//! "#;
//!
//! let sandbox = Sandbox::deny_all().unwrap();
//! let limits = Limits::new(1_000_000, Duration::from_millis(500), 1 << 20);
//! let out = sandbox
//!     .run(wat.as_bytes(), "add", &[Value::I32(2), Value::I32(40)], &limits)
//!     .unwrap();
//! assert_eq!(out.values, vec![Value::I32(42)]);
//! ```
//!
//! See the wiki for the threat model and the full design.

mod error;
mod host;
mod limits;
mod sandbox;

pub use error::{Result, SandboxError};
pub use host::{HostAbi, LogSink};
pub use limits::Limits;
pub use sandbox::{RunOutput, Sandbox, Value};
