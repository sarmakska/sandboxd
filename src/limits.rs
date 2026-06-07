//! Resource limit configuration and enforcement.
//!
//! The three limits are independent and each one closes a different escape
//! route for untrusted code:
//!
//! - `fuel` bounds the number of WebAssembly instructions executed. It is the
//!   deterministic CPU budget.
//! - `timeout` bounds wall-clock time using wasmtime's epoch interruption. It
//!   catches code that does not burn fuel predictably.
//! - `memory_bytes` bounds linear memory growth through a `ResourceLimiter`.

use std::time::Duration;

use wasmtime::{ResourceLimiter, StoreLimits, StoreLimitsBuilder};

/// The resource budget applied to a single sandboxed run.
///
/// Construct with [`Limits::new`] or tweak the defaults with the builder-style
/// `with_*` methods. The defaults are intentionally tight so that an embedder
/// who forgets to configure them still runs untrusted code under a cap.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Maximum WebAssembly instructions (fuel units) the guest may execute.
    pub fuel: u64,
    /// Maximum wall-clock duration before the epoch timer interrupts the guest.
    pub timeout: Duration,
    /// Maximum bytes of linear memory the guest may allocate.
    pub memory_bytes: usize,
    /// Maximum number of table elements the guest may allocate.
    pub table_elements: usize,
    /// Maximum number of concurrent instances the guest may create.
    pub instances: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            // One hundred million instructions is enough for meaningful work
            // while still terminating an infinite loop in a fraction of a
            // second.
            fuel: 100_000_000,
            // One second of wall-clock time by default.
            timeout: Duration::from_secs(1),
            // Sixteen mebibytes of linear memory by default.
            memory_bytes: 16 * 1024 * 1024,
            // A modest table cap; most well-behaved modules need very little.
            table_elements: 10_000,
            // A single instance unless the embedder opts into more.
            instances: 1,
        }
    }
}

impl Limits {
    /// Create a limit set from the three primary bounds, leaving table and
    /// instance caps at their defaults.
    pub fn new(fuel: u64, timeout: Duration, memory_bytes: usize) -> Self {
        Self {
            fuel,
            timeout,
            memory_bytes,
            ..Self::default()
        }
    }

    /// Override the fuel budget.
    pub fn with_fuel(mut self, fuel: u64) -> Self {
        self.fuel = fuel;
        self
    }

    /// Override the wall-clock timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the linear memory cap in bytes.
    pub fn with_memory_bytes(mut self, memory_bytes: usize) -> Self {
        self.memory_bytes = memory_bytes;
        self
    }

    /// Build a wasmtime [`StoreLimits`] that enforces the memory, table and
    /// instance caps. The fuel and timeout limits are enforced separately on
    /// the [`wasmtime::Store`] and engine.
    pub(crate) fn store_limits(&self) -> StoreLimits {
        StoreLimitsBuilder::new()
            .memory_size(self.memory_bytes)
            .table_elements(self.table_elements)
            .instances(self.instances)
            // Refuse the growth request rather than aborting the process. The
            // guest sees a failed `memory.grow` (a -1 return) and, in our
            // fixtures, traps on the following store.
            .trap_on_grow_failure(false)
            .build()
    }
}

/// The per-store state that holds the resource limiter.
///
/// wasmtime calls into the [`ResourceLimiter`] trait on every attempted memory
/// or table growth, which is where the cap is enforced.
pub struct StoreState {
    limits: StoreLimits,
    /// Set to true the first time a memory or table growth is denied by the
    /// cap. The sandbox reads this after a trap so it can report a memory
    /// limit breach precisely, even when the guest turns the failed growth
    /// into an `unreachable` trap rather than a memory access fault.
    growth_denied: bool,
    /// The largest linear-memory size, in bytes, the guest was ever allowed to
    /// reach during the run. Updated on every granted growth so the embedder
    /// can size the memory cap from one observed run.
    peak_memory_bytes: usize,
}

impl StoreState {
    pub(crate) fn new(limits: &Limits) -> Self {
        Self {
            limits: limits.store_limits(),
            growth_denied: false,
            peak_memory_bytes: 0,
        }
    }

    /// Whether the guest was denied a memory or table growth during the run.
    pub(crate) fn growth_was_denied(&self) -> bool {
        self.growth_denied
    }

    /// The high-water mark of linear memory the guest reached, in bytes.
    pub(crate) fn peak_memory_bytes(&self) -> usize {
        self.peak_memory_bytes
    }
}

impl ResourceLimiter for StoreState {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let allowed = self.limits.memory_growing(current, desired, maximum)?;
        if allowed {
            self.peak_memory_bytes = self.peak_memory_bytes.max(desired);
        } else {
            self.growth_denied = true;
        }
        Ok(allowed)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let allowed = self.limits.table_growing(current, desired, maximum)?;
        if !allowed {
            self.growth_denied = true;
        }
        Ok(allowed)
    }

    fn instances(&self) -> usize {
        self.limits.instances()
    }

    fn tables(&self) -> usize {
        self.limits.tables()
    }

    fn memories(&self) -> usize {
        self.limits.memories()
    }
}
