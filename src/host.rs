//! The host ABI: deny-by-default, explicit allow-list.
//!
//! There is no WASI and there are no ambient host functions. A freshly built
//! [`HostAbi`] exposes nothing at all, so a module that imports anything is
//! rejected at instantiation. The embedder opts in to each capability
//! explicitly. Today the only audited capability is `host::log`, which lets a
//! guest emit a UTF-8 string for observability without granting it any other
//! reach into the host.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use wasmtime::{bail, Caller, Error, Extern, Linker, Result};

use crate::limits::StoreState;

/// A captured log line emitted by the guest through `host::log`.
pub type LogSink = Arc<Mutex<Vec<String>>>;

/// Describes which host capabilities a sandboxed module is permitted to import.
///
/// The design is deliberately additive: the only way to widen the guest's
/// reach is to call an `allow_*` method. Nothing is granted implicitly.
#[derive(Clone, Default)]
pub struct HostAbi {
    /// When set, the guest may import `host::log`. Lines are appended to this
    /// sink so the embedder can audit exactly what the guest emitted.
    log_sink: Option<LogSink>,
    /// When set, the guest may import `host::random`. The generator is seeded
    /// and deterministic, so a run is as reproducible as a pure one: the same
    /// seed and the same call sequence yield the same numbers every time.
    rng_state: Option<Arc<AtomicU64>>,
}

impl HostAbi {
    /// A host ABI that grants nothing. Any imported function or memory causes
    /// instantiation to fail. This is the default and the safe baseline.
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Permit the guest to import `host::log` and capture every line it emits.
    ///
    /// The returned sink is shared with the running store; read it after the
    /// run to audit what the guest logged.
    pub fn allow_log(mut self) -> (Self, LogSink) {
        let sink: LogSink = Arc::new(Mutex::new(Vec::new()));
        self.log_sink = Some(sink.clone());
        (self, sink)
    }

    /// Whether the `host::log` capability has been granted.
    pub fn log_allowed(&self) -> bool {
        self.log_sink.is_some()
    }

    /// Permit the guest to import `host::random`, a deterministic 64-bit
    /// generator seeded by `seed`.
    ///
    /// The generator is a splitmix64 advance over an atomic counter, so it
    /// needs no external dependency and preserves the project's reproducibility
    /// guarantee: the same seed and the same number of calls produce the same
    /// stream every time. It is suitable for simulation, sampling and test
    /// fixtures. It is *not* a cryptographic source and must not be used to
    /// generate keys, nonces or anything where unpredictability matters.
    pub fn allow_random(mut self, seed: u64) -> Self {
        self.rng_state = Some(Arc::new(AtomicU64::new(seed)));
        self
    }

    /// Whether the `host::random` capability has been granted.
    pub fn random_allowed(&self) -> bool {
        self.rng_state.is_some()
    }

    /// Register the allowed imports onto a linker.
    ///
    /// Only capabilities that were explicitly granted are defined. Because we
    /// do not call [`Linker::define_unknown_imports_as_traps`] or anything
    /// similar, any import the module needs that we did not define here causes
    /// [`Linker::instantiate`] to fail. That failure is what enforces the
    /// deny-by-default contract.
    pub(crate) fn register(&self, linker: &mut Linker<StoreState>) -> Result<()> {
        if let Some(sink) = &self.log_sink {
            let sink = sink.clone();
            // Signature: (ptr: i32, len: i32) -> (). The guest writes a UTF-8
            // string into its own linear memory and passes the offset and
            // length. We copy it out and never hand the guest a pointer back,
            // so there is no path from this import to host memory.
            linker.func_wrap(
                "host",
                "log",
                move |mut caller: Caller<'_, StoreState>, ptr: i32, len: i32| -> Result<()> {
                    let line = read_guest_string(&mut caller, ptr, len)?;
                    sink.lock().expect("log sink mutex poisoned").push(line);
                    Ok(())
                },
            )?;
        }
        if let Some(state) = &self.rng_state {
            let state = state.clone();
            // Signature: () -> i64. Each call advances the seeded generator by
            // one splitmix64 step and returns the next 64-bit value. The guest
            // gets numbers but no reach into the host: there is no pointer, no
            // memory access and no syscall behind this.
            linker.func_wrap("host", "random", move |_caller: Caller<'_, StoreState>| {
                next_random(&state) as i64
            })?;
        }
        Ok(())
    }
}

/// Advance a splitmix64 generator held in an atomic and return the next value.
///
/// splitmix64 is a well-known finaliser: it passes the usual statistical test
/// suites for a non-cryptographic generator, has a full 2^64 period, and needs
/// no state beyond a single 64-bit counter. We use a relaxed
/// fetch-then-finalise so concurrent guests on a shared ABI still each get a
/// distinct, deterministic-per-seed value without a lock.
fn next_random(state: &AtomicU64) -> u64 {
    let z = state.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    let z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Read a UTF-8 string out of the guest's exported linear memory.
///
/// Bounds are validated against the actual memory size, so a malicious or
/// buggy guest that passes an out-of-range pointer or length gets a trap
/// rather than reading arbitrary host bytes. Invalid UTF-8 is replaced rather
/// than rejected so a noisy guest cannot abort the host with bad bytes.
fn read_guest_string(caller: &mut Caller<'_, StoreState>, ptr: i32, len: i32) -> Result<String> {
    let memory = match caller.get_export("memory") {
        Some(Extern::Memory(m)) => m,
        _ => bail!("host::log requires the guest to export its linear memory as `memory`"),
    };

    let ptr = usize::try_from(ptr).map_err(|_| Error::msg("negative pointer in host::log"))?;
    let len = usize::try_from(len).map_err(|_| Error::msg("negative length in host::log"))?;

    let data = memory.data(&caller);
    let end = ptr
        .checked_add(len)
        .ok_or_else(|| Error::msg("pointer plus length overflows in host::log"))?;
    let bytes = data
        .get(ptr..end)
        .ok_or_else(|| Error::msg("host::log pointer or length out of bounds"))?;

    Ok(String::from_utf8_lossy(bytes).into_owned())
}
