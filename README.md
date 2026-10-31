# hashwx

Rust implementation of [HashWX 1.0](https://github.com/tevador/hashwx), with a portable Rust execution path, seed-specialized native code generation, WebAssembly emitters, and a repo-owned HashX v1 implementation using the same backend model.

## Features

- `native` (default): x86-64 and AArch64 JIT backends using `dynasmrt` for both HashWX and HashX.
- `hashx` (default): repo-owned HashX v1 seed expansion, program generation, constraint/scheduling model, interpreter, and native JIT. The upstream `hashx` crate is dev-only and used as a differential oracle.
- `wasm`: emits self-contained HashWX WebAssembly and, with `hashx`, HashX WebAssembly. Both export `hash(i64) -> i64`; no Wasm runtime is required by this crate.
- Backend-neutral HashWX `ProgramSet` IR so additional native architectures can share the exact same HashWX program generator.

`HashWx::new` selects the native JIT on x86-64/AArch64 and the portable Rust engine elsewhere. `compile` remains the explicit native-only API, while `interpret` forces the portable engine:

```rust
let hashwx = hashwx::HashWx::new(&seed)?;
let value = hashwx.hash(nonce);

let portable = hashwx::HashWx::interpret(&seed);
assert_eq!(value, portable.hash(nonce));
```

To emit HashWX WebAssembly instead:

```rust
let module = hashwx::wasm::compile(&seed);
```

The generated module owns a fixed 64 KiB memory and imports nothing.

HashX uses the same execution split and exposes one immutable generated program for CPU, Wasm, and accelerator backends:

```rust
let program = hashwx::HashXProgram::generate(seed)?;
let hashx = hashwx::HashX::new(seed)?;
let value = hashx.hash(nonce);
let digest = hashx.hash_bytes(nonce);

let instructions = program.instructions();
let register_key = program.register_key();
let reference = hashwx::HashX::interpret_program(&program);
assert_eq!(value, reference.hash(nonce));

let wasm = hashwx::wasm::compile_hashx_program(&program);
```

`hashwx::wasm::compile_hashx(seed)` is the seed-taking convenience form. On native-capable targets, `HashX::compile_program(&program)` lets the CPU JIT consume that exact same generated program.

HashX keeps the exact v1 seed/program semantics, including the small set of seeds rejected by whole-program constraints. `HashX::compile` emits native code with hashwx-rs's own backend and never silently falls back; `HashX::new` is the target-selected convenience constructor.

## Correctness

The test suite includes the official HashWX 1.0 and HashX v1 vectors. The repo-owned HashX generator/interpreter is differentially checked against upstream `hashx` as a dev-only oracle, and the native HashX JIT is checked against our interpreter on the executing host. Generated HashWX and HashX WebAssembly are instantiated and executed against their official vectors and deterministic differential corpora. The portable HashWX engine is also checked against generated Wasm and native execution. The x86-64 native HashWX path is exercised directly in hosted CI; the AArch64 HashWX and HashX JITs are cross-built as static musl tests and executed under `qemu-aarch64` in the release gate. ARMv7 hard-float Linux executes the complete all-feature portable/runtime suite under `qemu-arm`.

## Performance

On the development x86-64 systems, the HashWX native backend generates roughly 9.4 KiB of machine code in tens of microseconds. The generated Wasm module is roughly 14 KiB. Performance depends strongly on CPU and Wasm engine; benchmarks are intentionally not presented as stable API guarantees.

`cargo run --release --example bench_native -- hashwx` benchmarks HashWX; the same command with `hashx` benchmarks the repo-owned HashX JIT. On the Ryzen 9 4900HS development machine the current x86-64 HashX path is roughly 14.6 MH/s, with about 70–90 µs warm JIT compilation. These numbers are development measurements, not API guarantees.

## Portability

The HashWX program generator and `ProgramSet` are architecture-neutral. The HashWX native backend supports x86-64 and AArch64. The release gate cross-compiles AArch64 and executes the native HashWX/HashX test binaries under `qemu-aarch64`. ARMv7 (`armv7-unknown-linux-musleabihf`) is a tested portable target: the all-feature test suite executes under `qemu-arm`, including portable HashWX, the HashX interpreter, and both Wasm emitters. The `wasm` backend is available independently with `--no-default-features --features wasm`.

HashWX and HashX have portable Rust execution on every supported Rust target. With `native`, `HashWx::new` and `HashX::new` select hashwx-rs JIT backends on x86-64/AArch64 and otherwise select the portable engine; explicit `compile` remains native-only. ARMv7 therefore has complete functional support without an ARM32 JIT. The `wasm` feature can emit HashWX on its own and HashX when combined with `hashx`.

## License

LGPL-3.0-only. HashWX is LGPL-3.0-or-later, while the in-tree HashX compatibility model is derived from the Tor Project Arti `hashx` 0.10.0 implementation under LGPL-3.0-only, so the combined crate uses the common LGPL-3.0-only terms. hashwx-rs provides its own HashX execution backends; upstream `hashx` remains a dev-only differential oracle.
