# Changelog

## 0.1.0 - unreleased

- Implement HashWX 1.0 program generation with x86-64/AArch64 native JIT backends.
- Add a portable HashWX WebAssembly emitter.
- Implement HashX v1 in-tree, including generation, interpreter, constraints/scheduling, and repo-owned x86-64/AArch64 JIT backends.
- Differentially test HashX generation and execution against upstream `hashx` 0.10 as a dev-only oracle.
- Expose generated HashWX IR through read-only accessors so native JIT invariants cannot be violated through the safe public API.
- Expose immutable generated HashX programs so CPU and accelerator backends can share one authoritative instruction stream and register key.
- Execute generated WebAssembly against the official HashWX vectors in tests.
- Add reproducible locked CI, strict rustdoc/public-API checks, Windows cross-checking, and verified package/publish dry runs.
- Execute AArch64 native JIT vector/differential tests under QEMU and expand deterministic generator/backend corpus coverage.
- Execute the portable HashX/HashWX/Wasm paths on ARMv7 hard-float Linux under QEMU and keep an all-features ARMv7 cross-check in CI.
