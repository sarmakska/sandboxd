//! The sandbox engine: compile, instantiate under limits, and run.
//!
//! A [`Sandbox`] owns a configured wasmtime [`Engine`]. Each call to
//! [`Sandbox::run`] builds a fresh [`Store`] so that runs share no state. The
//! engine is configured once with fuel consumption and epoch interruption
//! enabled; the per-run budget is applied to the store.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use wasmtime::{Config, Engine, Linker, Module, Store, Val, ValType};

use crate::error::{Result, SandboxError};
use crate::host::HostAbi;
use crate::limits::{Limits, StoreState};

/// The value a guest function returns, narrowed to the scalar types our ABI
/// supports. References and v128 are intentionally excluded from the public
/// surface to keep the boundary small and auditable.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl Value {
    fn from_val(v: &Val) -> Result<Self> {
        match v {
            Val::I32(x) => Ok(Value::I32(*x)),
            Val::I64(x) => Ok(Value::I64(*x)),
            Val::F32(bits) => Ok(Value::F32(f32::from_bits(*bits))),
            Val::F64(bits) => Ok(Value::F64(f64::from_bits(*bits))),
            other => Err(SandboxError::Export(format!(
                "unsupported return type {other:?}"
            ))),
        }
    }

    fn to_val(&self) -> Val {
        match self {
            Value::I32(x) => Val::I32(*x),
            Value::I64(x) => Val::I64(*x),
            Value::F32(x) => Val::F32(x.to_bits()),
            Value::F64(x) => Val::F64(x.to_bits()),
        }
    }
}

/// The outcome of a successful run.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// The values returned by the invoked export.
    pub values: Vec<Value>,
    /// Fuel consumed during the run, if the engine reports it.
    pub fuel_consumed: Option<u64>,
}

/// A reusable sandbox bound to a single engine configuration.
///
/// The engine is cheap to clone (it is reference counted internally) and is
/// safe to share across threads. Create one per process and call
/// [`Sandbox::run`] as many times as you like.
pub struct Sandbox {
    engine: Engine,
    host: HostAbi,
}

impl Sandbox {
    /// Build a sandbox with the given host ABI. The engine is configured to
    /// support both fuel metering and epoch interruption.
    pub fn new(host: HostAbi) -> Result<Self> {
        let mut config = Config::new();
        // Enable the deterministic instruction budget.
        config.consume_fuel(true);
        // Enable cooperative wall-clock interruption. We bump the epoch from a
        // watchdog thread; the guest is interrupted at the next safe point.
        config.epoch_interruption(true);
        // Cranelift is the default, but we set the optimisation level
        // explicitly for predictable startup behaviour.
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);

        let engine = Engine::new(&config)
            .map_err(|e| SandboxError::Host(format!("failed to build engine: {e}")))?;

        Ok(Self { engine, host })
    }

    /// Build a sandbox that grants nothing to the guest.
    pub fn deny_all() -> Result<Self> {
        Self::new(HostAbi::deny_all())
    }

    /// Borrow the engine, for example to validate fixtures in tests.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compile a module from `.wasm` bytes or `.wat` source.
    ///
    /// wasmtime parses WAT directly, so either form is accepted. Compilation
    /// failures surface as [`SandboxError::InvalidModule`].
    pub fn compile(&self, bytes: &[u8]) -> Result<Module> {
        Module::new(&self.engine, bytes).map_err(|e| SandboxError::InvalidModule(e.to_string()))
    }

    /// Instantiate and invoke an exported function under the configured limits.
    ///
    /// The flow is: compile, check imports against the allow-list (implicitly,
    /// by only defining permitted imports on the linker), apply the fuel,
    /// memory and epoch budgets to a fresh store, arm the watchdog, then call
    /// the export. Any limit breach is mapped to the matching
    /// [`SandboxError`] variant.
    pub fn run(
        &self,
        bytes: &[u8],
        func: &str,
        params: &[Value],
        limits: &Limits,
    ) -> Result<RunOutput> {
        let module = self.compile(bytes)?;

        // Surface disallowed imports as a precise, actionable error before we
        // even build the store. wasmtime would also reject them at
        // instantiation, but inspecting the module first lets us name the
        // offending import.
        self.reject_disallowed_imports(&module)?;

        let mut store = Store::new(&self.engine, StoreState::new(limits));
        store.limiter(|state| state);

        // Apply the deterministic CPU budget.
        store
            .set_fuel(limits.fuel)
            .map_err(|e| SandboxError::Host(format!("failed to set fuel: {e}")))?;

        // Arm epoch interruption. The store is interrupted after a single
        // epoch tick; the watchdog thread bumps the engine epoch once the
        // deadline elapses.
        store.set_epoch_deadline(1);

        let mut linker: Linker<StoreState> = Linker::new(&self.engine);
        self.host
            .register(&mut linker)
            .map_err(|e| SandboxError::Host(format!("failed to register host ABI: {e}")))?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| map_instantiation_error(e, &module, &self.host))?;

        let function = instance
            .get_func(&mut store, func)
            .ok_or_else(|| SandboxError::Export(format!("no exported function named `{func}`")))?;

        // Validate the parameter arity and types against the function so we
        // give a clean error rather than a panic on a mismatch.
        self.check_signature(&store, &function, params, func)?;

        let ty = function.ty(&store);
        let mut results: Vec<Val> = ty.results().map(default_val).collect();
        let call_params: Vec<Val> = params.iter().map(Value::to_val).collect();

        let watchdog = Watchdog::arm(self.engine.clone(), limits.timeout);

        let call_result = function.call(&mut store, &call_params, &mut results);

        watchdog.disarm();

        let fuel_consumed = store
            .get_fuel()
            .ok()
            .map(|remaining| limits.fuel.saturating_sub(remaining));

        match call_result {
            Ok(()) => {
                let values = results
                    .iter()
                    .map(Value::from_val)
                    .collect::<Result<Vec<_>>>()?;
                Ok(RunOutput {
                    values,
                    fuel_consumed,
                })
            }
            Err(trap) => {
                // If the guest was denied a memory or table growth at any
                // point, attribute the trap to the memory cap regardless of
                // how the guest reacted to the failed growth (it might trap
                // with `unreachable`, an out-of-bounds access, or a store).
                if store.data().growth_was_denied() {
                    return Err(SandboxError::MemoryLimitExceeded {
                        limit: limits.memory_bytes,
                    });
                }
                Err(classify_trap(trap, limits))
            }
        }
    }

    /// Inspect every import the module declares and reject any that the host
    /// ABI did not grant. This produces a [`SandboxError::DisallowedImport`]
    /// naming the exact import.
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

    fn check_signature(
        &self,
        store: &Store<StoreState>,
        function: &wasmtime::Func,
        params: &[Value],
        name: &str,
    ) -> Result<()> {
        let ty = function.ty(store);
        let expected: Vec<ValType> = ty.params().collect();
        if expected.len() != params.len() {
            return Err(SandboxError::Export(format!(
                "function `{name}` expects {} parameter(s) but {} were supplied",
                expected.len(),
                params.len()
            )));
        }
        for (i, (want, got)) in expected.iter().zip(params.iter()).enumerate() {
            let ok = matches!(
                (want, got),
                (ValType::I32, Value::I32(_))
                    | (ValType::I64, Value::I64(_))
                    | (ValType::F32, Value::F32(_))
                    | (ValType::F64, Value::F64(_))
            );
            if !ok {
                return Err(SandboxError::Export(format!(
                    "function `{name}` parameter {i} has type {want:?} but the supplied value does not match"
                )));
            }
        }
        Ok(())
    }
}

/// A watchdog thread that bumps the engine epoch once a deadline elapses.
///
/// wasmtime's epoch interruption is cooperative: the guest checks the epoch at
/// loop back-edges and function entries. By incrementing the epoch after the
/// timeout we force the next check to interrupt the guest. We keep this in its
/// own thread so a guest spinning in pure computation is still stopped.
struct Watchdog {
    done: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Watchdog {
    fn arm(engine: Engine, timeout: Duration) -> Self {
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done_clone = done.clone();
        let handle = thread::spawn(move || {
            // Poll in small slices so we can exit promptly when the run
            // finishes before the deadline, rather than always sleeping the
            // full timeout.
            let slice = Duration::from_millis(5).min(timeout.max(Duration::from_millis(1)));
            let mut elapsed = Duration::ZERO;
            while elapsed < timeout {
                if done_clone.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                thread::sleep(slice);
                elapsed += slice;
            }
            // Deadline reached: bump the epoch to interrupt the guest.
            engine.increment_epoch();
        });
        Self {
            done,
            handle: Some(handle),
        }
    }

    fn disarm(mut self) {
        self.done.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn default_val(ty: ValType) -> Val {
    match ty {
        ValType::I32 => Val::I32(0),
        ValType::I64 => Val::I64(0),
        ValType::F32 => Val::F32(0),
        ValType::F64 => Val::F64(0),
        _ => Val::I32(0),
    }
}

/// Map an instantiation failure to a typed error. A missing import is the most
/// common cause and means the module wanted a capability the host did not
/// grant.
fn map_instantiation_error(err: wasmtime::Error, module: &Module, host: &HostAbi) -> SandboxError {
    let message = err.to_string();
    if message.contains("unknown import") || message.contains("incompatible import") {
        for import in module.imports() {
            let allowed = matches!(
                (import.module(), import.name(), host.log_allowed()),
                ("host", "log", true)
            );
            if !allowed {
                return SandboxError::DisallowedImport {
                    module: import.module().to_string(),
                    name: import.name().to_string(),
                };
            }
        }
    }
    SandboxError::Host(format!("instantiation failed: {message}"))
}

/// Classify a trap raised during the call. Fuel and epoch interruption have
/// dedicated trap codes; memory growth failures surface as ordinary traps once
/// the guest acts on the failed `memory.grow`.
fn classify_trap(err: wasmtime::Error, limits: &Limits) -> SandboxError {
    use wasmtime::Trap;

    if let Some(trap) = err.downcast_ref::<Trap>() {
        match trap {
            Trap::OutOfFuel => {
                return SandboxError::FuelExhausted {
                    budget: limits.fuel,
                }
            }
            Trap::Interrupt => {
                return SandboxError::Timeout {
                    millis: limits.timeout.as_millis() as u64,
                }
            }
            Trap::MemoryOutOfBounds | Trap::TableOutOfBounds => {
                return SandboxError::MemoryLimitExceeded {
                    limit: limits.memory_bytes,
                }
            }
            _ => {}
        }
    }
    SandboxError::Trap(err.to_string())
}
