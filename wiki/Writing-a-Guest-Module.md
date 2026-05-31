# Writing a Guest Module

How to produce a WebAssembly module that sandboxd will accept and run. The short version: compile for a freestanding target, export the function you want to call, and import nothing you have not been granted. The detail follows.

## The contract, restated for a module author

sandboxd will run your module if it meets the contract from [Module Format and WAT](Module-Format-and-WAT):

1. It exports the function the embedder invokes (the CLI default is `run`).
2. That function's parameters are scalars (`i32`, `i64`, `f32`, `f64`) matching what the caller passes.
3. It imports nothing the host did not grant. The default grants nothing.
4. If it uses `host::log`, it exports linear memory as `memory`.
5. It stays within the fuel, time and memory limits.

The single most common reason a real-language module is rejected is point 3: the standard library pulled in WASI imports. The fix is the target you compile for.

## Compile for `wasm32-unknown-unknown`, not `wasm32-wasi`

A guest built for `wasm32-wasi` imports WASI functions like `wasi_snapshot_preview1::fd_write`, which sandboxd does not provide. It is rejected at instantiation with `DisallowedImport`. Build for the freestanding `wasm32-unknown-unknown` target so the module imports nothing.

```bash
rustup target add wasm32-unknown-unknown
```

## A minimal Rust guest

```rust
// lib.rs of the guest crate, compiled as a cdylib for wasm32-unknown-unknown.
#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[no_mangle]
pub extern "C" fn fib(n: i32) -> i32 {
    let (mut a, mut b) = (0i32, 1i32);
    for _ in 0..n {
        let t = a + b;
        a = b;
        b = t;
    }
    a
}
```

```toml
# Cargo.toml of the guest crate
[lib]
crate-type = ["cdylib"]
```

```bash
cargo build --release --target wasm32-unknown-unknown
# emits target/wasm32-unknown-unknown/release/<name>.wasm
```

The `#![no_std]` and the custom panic handler keep the module free of the standard library and its WASI imports. `#[no_mangle]` and `extern "C"` give the exports stable names. Run it:

```bash
sandboxd target/wasm32-unknown-unknown/release/myguest.wasm --invoke add --arg 2 --arg 40
```

## Using the log capability from a guest

If the embedder grants `host::log`, your guest can emit observability lines. The import signature is `(ptr: i32, len: i32) -> ()`. You write a UTF-8 string into your own linear memory and pass the offset and length. You must export your memory as `memory`.

In WAT, exactly as the `logger.wat` fixture does it:

```wat
(module
  (import "host" "log" (func $log (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "hello from the guest")
  (func (export "run")
    (call $log (i32.const 0) (i32.const 20))))
```

From Rust you would declare the import and call it with a pointer into a static or stack buffer:

```rust
extern "C" {
    fn log(ptr: i32, len: i32);
}

#[no_mangle]
pub extern "C" fn run() {
    let msg = b"hello from the guest";
    unsafe { log(msg.as_ptr() as i32, msg.len() as i32); }
}
```

The link section must place this import under module `host`, field `log`. In Rust that is the default for a plain `extern "C"` block when the symbol is named `log` and you set the import module via `#[link(wasm_import_module = "host")]`:

```rust
#[link(wasm_import_module = "host")]
extern "C" {
    fn log(ptr: i32, len: i32);
}
```

The host validates `ptr` and `ptr + len` against your actual memory size before reading, so an out-of-range pointer traps cleanly rather than reading host memory. See [Host ABI](Host-ABI).

## Hand-writing WAT

For tests, demos and small kernels, WAT is often easier than a full toolchain, and sandboxd parses it directly. The fixtures in `fixtures/` are the reference. WAT is the s-expression text form of WebAssembly; the [WebAssembly text format spec](https://webassembly.github.io/spec/core/text/index.html) is the authority, and `wat2wasm` from the WebAssembly Binary Toolkit can validate it offline.

```wat
(module
  (func (export "double") (param i32) (result i32)
    local.get 0
    i32.const 2
    i32.mul))
```

```bash
sandboxd double.wat --invoke double --arg 21   # result: I32(42)
```

## Keeping within the limits

- **Fuel:** each instruction costs fuel. Tight loops are fine; unbounded loops are not. Profile your worst input and ask the embedder for a budget with headroom (see [Configuration and Tuning](Configuration-and-Tuning)).
- **Memory:** linear memory grows in 64 KiB pages and is capped. Allocate what you need up front rather than growing unbounded; an unbounded `memory.grow` loop is the memory-bomb attack and will be stopped.
- **Time:** if the embedder grants a capability you call into, remember the wall-clock deadline still applies to time spent there.

## Common rejection causes

```mermaid
%%{init: {'theme':'base','themeVariables':{'primaryColor':'#0d1117','primaryTextColor':'#f5f7fa','primaryBorderColor':'#38bdf8','lineColor':'#22d3ee','secondaryColor':'#0f172a','tertiaryColor':'#0d1117','fontFamily':'ui-monospace, monospace'}}}%%
flowchart TD
    A[module rejected] --> B{error}
    B -->|DisallowedImport wasi_snapshot_preview1| C[built for wasm32-wasi: rebuild for unknown-unknown]
    B -->|DisallowedImport env::*| D[std or a crate pulled in a host import: go no_std]
    B -->|Export no such function| E[wrong export name or missing no_mangle]
    B -->|InvalidModule| F[not valid wasm/wat or truncated]
    B -->|host::log requires memory export| G[add (memory (export "memory") ...)]
```

---
SarmaLinux . sarmalinux.com . [repo](https://github.com/sarmakska/sandboxd)
