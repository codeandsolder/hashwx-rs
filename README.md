# hashwx

Rust implementation of [HashWX 1.0](https://github.com/tevador/hashwx), with seed-specialized native code generation, a portable WebAssembly code-generation backend, and a repo-owned HashX v1 mode using the same native JIT infrastructure.

## Features

- `native` (default): x86-64 and AArch64 JIT backends using `dynasmrt` for both HashWX and HashX.
- `hashx` (default): repo-owned HashX v1 seed expansion, program generation, constraint/scheduling model, interpreter, and native JIT. The upstream `hashx` crate is dev-only and used as a differential oracle.
- `wasm`: emits a self-contained HashWX WebAssembly module exporting `hash(i64) -> i64`; no Wasm runtime is required by this crate.
- Backend-neutral HashWX `ProgramSet` IR so additional native architectures can share the exact same HashWX program generator.

The compiled HashWX object is immutable and can be shared between threads. Scratch memory lives on the generated function's stack, so hashing is simply:

```rust
let hashwx = hashwx::HashWx::compile(&seed)?;
let value = hashwx.hash(nonce);
```

To emit HashWX WebAssembly instead:

```rust
let module = hashwx::wasm::compile(&seed);
```

The generated module owns a fixed 64 KiB memory and imports nothing.

HashX uses the same split between native code generation and a portable reference path:

```rust
let native = hashwx::HashX::compile(seed)?;
let value = native.hash(nonce);
let digest = native.hash_bytes(nonce);

let reference = hashwx::HashX::interpret(seed)?;
assert_eq!(value, reference.hash(nonce));
```

HashX keeps the exact v1 seed/program semantics, including the small set of seeds rejected by whole-program constraints. `HashX::compile` emits native code with hashwx-rs's own backend and never silently falls back to the interpreter.

## Correctness

The test suite includes the official HashWX 1.0 and HashX v1 vectors. The repo-owned HashX generator/interpreter is differentially checked against upstream `hashx` as a dev-only oracle, and the native HashX JIT is checked against our interpreter on the executing host. Generated HashWX WebAssembly is instantiated and executed against the official vectors. The x86-64 native HashWX path is exercised directly in hosted CI; the AArch64 HashWX and HashX JITs are cross-built as static musl tests and executed under `qemu-aarch64` in the release gate.

## Performance

On the development x86-64 systems, the HashWX native backend generates roughly 9.4 KiB of machine code in tens of microseconds. The generated Wasm module is roughly 14 KiB. Performance depends strongly on CPU and Wasm engine; benchmarks are intentionally not presented as stable API guarantees.

`cargo run --release --example bench_native -- hashwx` benchmarks HashWX; the same command with `hashx` benchmarks the repo-owned HashX JIT. On the Ryzen 9 4900HS development machine the current x86-64 HashX path is roughly 14.6 MH/s, with about 70–90 µs warm JIT compilation. These numbers are development measurements, not API guarantees.

## Portability

The HashWX program generator and `ProgramSet` are architecture-neutral. The HashWX native backend supports x86-64 and AArch64. The release gate cross-compiles AArch64 and executes the native HashWX/HashX test binaries under `qemu-aarch64`. The `wasm` backend is available independently with `--no-default-features --features wasm`.

HashX interpretation is portable with `--no-default-features --features hashx`. Native HashX compilation currently uses hashwx-rs backends on x86-64 and AArch64 with `native,hashx`.

## License

LGPL-3.0-only. HashWX is LGPL-3.0-or-later, while the in-tree HashX compatibility model is derived from the Tor Project Arti `hashx` 0.10.0 implementation under LGPL-3.0-only, so the combined crate uses the common LGPL-3.0-only terms. hashwx-rs provides its own HashX execution backends; upstream `hashx` remains a dev-only differential oracle.
