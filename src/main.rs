//! The `sandboxd` command-line interface.
//!
//! Run an untrusted `.wasm` or `.wat` module under explicit CPU, wall-clock
//! and memory limits with a deny-by-default host ABI.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use sandboxd::{HostAbi, Limits, Sandbox, SandboxError, Value};

/// Run untrusted WebAssembly under hard resource limits.
#[derive(Parser, Debug)]
#[command(
    name = "sandboxd",
    version,
    about = "Run untrusted WebAssembly under CPU, wall-clock and memory limits with a deny-by-default host ABI.",
    long_about = None
)]
struct Cli {
    /// Path to the `.wasm` or `.wat` module to run.
    module: PathBuf,

    /// Name of the exported function to invoke.
    #[arg(short, long, default_value = "run")]
    invoke: String,

    /// Fuel budget: the maximum number of WebAssembly instructions executed.
    #[arg(long, default_value_t = 100_000_000)]
    fuel: u64,

    /// Wall-clock timeout in milliseconds.
    #[arg(long, default_value_t = 1000)]
    timeout_ms: u64,

    /// Linear memory cap in mebibytes.
    #[arg(long, default_value_t = 16)]
    memory_mb: usize,

    /// i32 arguments to pass to the function, in order.
    #[arg(long = "arg", value_name = "I32")]
    args: Vec<i32>,

    /// Grant the audited `host::log` capability so the guest can emit log
    /// lines. Off by default: the host denies everything unless asked.
    #[arg(long)]
    allow_log: bool,

    /// Grant the audited `host::random` capability, seeded with this value, so
    /// the guest can draw deterministic 64-bit numbers. Off by default. The
    /// generator is reproducible per seed and is not cryptographic.
    #[arg(long, value_name = "SEED")]
    seed: Option<u64>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("sandboxd: {err}");
            // Map the error category to a distinct exit code so scripts can
            // branch on why a run stopped.
            ExitCode::from(exit_code_for(&err))
        }
    }
}

fn run(cli: Cli) -> Result<(), SandboxError> {
    let bytes = std::fs::read(&cli.module)
        .map_err(|e| SandboxError::Host(format!("cannot read {}: {e}", cli.module.display())))?;

    let limits = Limits::new(
        cli.fuel,
        Duration::from_millis(cli.timeout_ms),
        cli.memory_mb * 1024 * 1024,
    );

    let (mut host, log_sink) = if cli.allow_log {
        let (host, sink) = HostAbi::deny_all().allow_log();
        (host, Some(sink))
    } else {
        (HostAbi::deny_all(), None)
    };
    if let Some(seed) = cli.seed {
        host = host.allow_random(seed);
    }

    let sandbox = Sandbox::new(host)?;

    let params: Vec<Value> = cli.args.iter().map(|a| Value::I32(*a)).collect();

    let output = sandbox.run(&bytes, &cli.invoke, &params, &limits)?;

    if let Some(sink) = log_sink {
        let lines = sink.lock().expect("log sink mutex poisoned");
        for line in lines.iter() {
            println!("[guest log] {line}");
        }
    }

    if output.values.is_empty() {
        println!("ok (no return value)");
    } else {
        let rendered: Vec<String> = output.values.iter().map(|v| format!("{v:?}")).collect();
        println!("result: {}", rendered.join(", "));
    }
    if let Some(fuel) = output.fuel_consumed {
        eprintln!("fuel consumed: {fuel}");
    }
    if output.peak_memory_bytes > 0 {
        eprintln!("peak linear memory: {} bytes", output.peak_memory_bytes);
    }

    Ok(())
}

fn exit_code_for(err: &SandboxError) -> u8 {
    match err {
        SandboxError::FuelExhausted { .. } => 2,
        SandboxError::Timeout { .. } => 3,
        SandboxError::MemoryLimitExceeded { .. } => 4,
        SandboxError::DisallowedImport { .. } => 5,
        SandboxError::InvalidModule(_) => 6,
        SandboxError::Export(_) => 7,
        SandboxError::Trap(_) => 8,
        SandboxError::Host(_) => 1,
    }
}
