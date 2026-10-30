# hashwx

Rust implementation of [HashWX 1.0](https://github.com/tevador/hashwx), with seed-specialized native code generation, a portable WebAssembly code-generation backend, and an upstream-compatible HashX mode for direct comparison and integration.

## Features

- `native` (default): x86-64 and AArch64 HashWX JITs using `dynasmrt`; when `hashx` is enabled it also enables upstream HashX native compilation.
- `hashx` (default): exposes HashX v1 through the maintained upstream Rust implementation instead of duplicating its scheduler/constraint generator.
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

For HashX, the same crate provides explicit native and reference paths:

```rust
let native = hashwx::HashX::compile(seed)?;
let value = native.hash(nonce);
let digest = native.hash_bytes(nonce);

let reference = hashwx::HashX::interpret(seed)?;
assert_eq!(value, reference.hash(nonce));
```

HashX keeps upstream seed semantics, including the small set of seeds rejected by its program constraints. `HashX::compile` never silently falls back to the interpreter.

## Correctness

The test suite includes the official HashWX 1.0 reference vectors and the upstream HashX v1 vectors. HashX native code is checked against the upstream interpreter, while HashWX native x86-64/AArch64 and generated WebAssembly are checked against the HashWX reference implementation.

## Performance

On the development x86-64 systems, the HashWX native backend generates roughly 9.4 KiB of machine code in tens of microseconds. The generated Wasm module is roughly 14 KiB. Performance depends strongly on CPU and Wasm engine; benchmarks are intentionally not presented as stable API guarantees.

`cargo run --release --example bench_native -- hashwx` benchmarks HashWX and the same command with `hashx` benchmarks the upstream HashX JIT through this crate.

## Portability

The HashWX program generator and `ProgramSet` are architecture-neutral. The HashWX native backend supports x86-64 and AArch64. AArch64 is tested against the official vectors under QEMU; real-hardware benchmarking is still intentionally boringly pending. The `wasm` backend is available independently with `--no-default-features --features wasm`.

HashX interpretation is portable with `--no-default-features --features hashx`. Native HashX compilation is available with `native,hashx` on the architectures supported by upstream HashX.

## License

LGPL-3.0-or-later, matching upstream HashWX. Derived tables and algorithm implementation are based on tevador/hashwx. HashX is used as an LGPL-3.0-only dependency from the maintained Arti implementation.
