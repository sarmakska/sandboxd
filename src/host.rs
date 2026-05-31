//! The host ABI: deny-by-default, explicit allow-list.
//!
//! There is no WASI and there are no ambient host functions. A freshly built
//! [`HostAbi`] exposes nothing at all, so a module that imports anything is
//! rejected at instantiation. The embedder opts in to each capability
//! explicitly. Today the only audited capability is `host::log`, which lets a
//! guest emit a UTF-8 string for observability without granting it any other
//! reach into the host.

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
        Ok(())
    }
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
