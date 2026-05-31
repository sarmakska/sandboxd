# CLI Usage

The `sandboxd` binary runs a single `.wasm` or `.wat` module under the limits you specify and prints the result. It is the quickest way to try a module without writing any Rust.

## Building

```bash
cargo build --release
# binary at ./target/release/sandboxd
```

The first build compiles wasmtime and is slow; later builds are fast.

## Synopsis

```
sandboxd <MODULE> [OPTIONS]
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `<MODULE>` | required | path to the `.wasm` or `.wat` module |
| `-i, --invoke <NAME>` | `run` | the exported function to call |
| `--fuel <N>` | `100000000` | instruction budget |
| `--timeout-ms <MS>` | `1000` | wall-clock deadline in milliseconds |
| `--memory-mb <MB>` | `16` | linear memory cap in mebibytes |
| `--arg <I32>` | none | an i32 argument; repeat for multiple, in order |
| `--allow-log` | off | grant the audited `host::log` capability |
| `-h, --help` | | print help |
| `-V, --version` | | print version |

## Exit codes

Each failure category exits with a distinct code so scripts can branch on why a run stopped:

| Code | Meaning |
| --- | --- |
| `0` | success |
| `1` | host or I/O error (for example the file could not be read) |
| `2` | fuel exhausted |
| `3` | wall-clock timeout |
| `4` | memory limit exceeded |
| `5` | disallowed import |
| `6` | invalid module |
| `7` | export error (missing function or signature mismatch) |
| `8` | guest trap |

## Worked examples

These all use the fixtures shipped in `fixtures/`.

### A well-behaved module

```bash
$ sandboxd fixtures/well_behaved.wat --invoke add --arg 2 --arg 40
result: I32(42)
fuel consumed: 4        # printed to stderr
```

```bash
$ sandboxd fixtures/well_behaved.wat --invoke fib --arg 30
result: I32(832040)
fuel consumed: 522
```

### Fuel exhaustion

```bash
$ sandboxd fixtures/infinite_loop.wat --fuel 1000000
sandboxd: fuel exhausted: the module exceeded its instruction budget of 1000000 units
$ echo $?
2
```

### Wall-clock timeout

Give it effectively unlimited fuel and a short deadline; the watchdog stops it.

```bash
$ sandboxd fixtures/infinite_loop.wat --fuel 100000000000 --timeout-ms 100
sandboxd: wall-clock timeout: the module ran longer than 100 ms
$ echo $?
3
```

### Memory cap

```bash
$ sandboxd fixtures/memory_bomb.wat --memory-mb 4 --fuel 1000000000
sandboxd: memory limit exceeded: the module requested more than 4194304 bytes of linear memory
$ echo $?
4
```

### Disallowed import

```bash
$ sandboxd fixtures/disallowed_import.wat
sandboxd: disallowed import: the module imports `env::secret` which is not on the allow-list
$ echo $?
5
```

### The log capability

By default even `host::log` is denied:

```bash
$ sandboxd fixtures/logger.wat
sandboxd: disallowed import: the module imports `host::log` which is not on the allow-list
$ echo $?
5
```

Grant it explicitly and the guest's line is captured and printed:

```bash
$ sandboxd fixtures/logger.wat --allow-log
[guest log] hello from the guest
ok (no return value)
fuel consumed: 4
```

## Notes

- Arguments are i32 only on the CLI. For other types, or for returning structured data, use the library API.
- `fuel consumed` is printed to stderr so it does not interfere with parsing the result on stdout.
- The default export name is `run`, which is why most fixtures export `run`; pass `--invoke` to call a different one.
