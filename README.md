# hashwx

Rust implementation of [HashWX 1.0](https://github.com/tevador/hashwx), with seed-specialized native code generation and a portable WebAssembly code-generation backend.

## Features

- `native` (default): x86-64 and AArch64 JITs using `dynasmrt`.
- `wasm`: emits a self-contained WebAssembly module exporting `hash(i64) -> i64`; no Wasm runtime is required by this crate.
- Backend-neutral `ProgramSet` IR so additional native architectures can share the exact same HashWX program generator.

The compiled native object is immutable and can be shared between threads. Scratch memory lives on the generated function's stack, so hashing is simply:

```rust
let hashwx = hashwx::HashWx::compile(&seed)?;
let value = hashwx.hash(nonce);
```

To emit WebAssembly instead:

```rust
let module = hashwx::wasm::compile(&seed);
```

The generated module owns a fixed 64 KiB memory and imports nothing.

## Correctness

The test suite includes the official HashWX 1.0 reference vectors. Native x86-64 and the generated WebAssembly have also been differential-tested against the upstream implementation.

## Performance

On the development x86-64 systems, the native backend generates roughly 9.4 KiB of machine code in tens of microseconds. The generated Wasm module is roughly 14 KiB. Performance depends strongly on CPU and Wasm engine; benchmarks are intentionally not presented as stable API guarantees.

## Portability

The HashWX program generator and `ProgramSet` are architecture-neutral. The native backend supports x86-64 and AArch64. AArch64 is tested against the official vectors under QEMU; real-hardware benchmarking is still intentionally boringly pending. The `wasm` backend is available independently with `--no-default-features --features wasm`.

## License

LGPL-3.0-or-later, matching upstream HashWX. Derived tables and algorithm implementation are based on tevador/hashwx.
