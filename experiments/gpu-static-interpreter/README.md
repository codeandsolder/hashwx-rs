# HashWX CUDA static-interpreter experiment (SM75)

This prototype is an architecture-level recreation of the *non-public* static
CUDA interpreter described in tevador/hashwx/doc/design.md, not a claim of
equivalent performance or production readiness.

## Algorithm and hardware

- One HashWX ProgramSet per active CUDA lane and 16,392 bytes private scratch.
- Program instructions packed to 16-bit micro-ops in shared memory, each
  32-bit branch-flag set compacted into one word. VM registers remain in
  lane-interleaved shared memory; 32 threads per block with configurable
  active lanes.
- Misaligned 64-bit scratch loads are reconstructed from two aligned loads.
- CPU generator uses repository HashWx::interpret_program as an independent
  oracle. The CUDA harness verifies 180 seeds and 4 nonces per seed (720 total
  comparisons) before measuring 463 hashes per seed.
- NVIDIA RTX 2060 Max-Q: 30 SMs, 3 MiB L2 and **currently 30 W power ceiling**.
  tevador's published 5.8 MH/s used RTX 5060 Ti, 32 MiB L2, 180 W and
  63 blocks x 32 lanes. These are not equivalent GPUs.

## Reproducibility

The separate fixture crate lives at fixture/Cargo.toml. Use the normal
nightly-2026-11-03 Cargo+sccache wrapper to generate 1024 fixture programs:

    cargo +nightly-2026-11-03 run --release --manifest-path fixture/Cargo.toml -- 1024 /home/jan/benchmarks/hashwx-gpu-static-interpreter-20261113/data

Build the SM75 kernel with the existing CUDA toolkit 13.4 into the existing
benchmark working directory, not a repository build artifact:

    /opt/cuda/bin/nvcc -std=c++17 -O3 -arch=sm_75 -Xptxas=-v static_interpreter.cu -o /home/jan/benchmarks/hashwx-gpu-static-interpreter-20261113/static_interpreter-verified -lcudart

Run after building:

    /home/jan/benchmarks/hashwx-gpu-static-interpreter-20261113/static_interpreter-verified /home/jan/benchmarks/hashwx-gpu-static-interpreter-20261113/data

## Measured 2026-11-13

- Original 32-bit shared IR: 39.2 KiB/block, max 0.0829 MH/s at 24 blocks
  x 8 lanes; at 30x8 the larger-than-L2 scratch set regresses to 0.0559.
- Compact 16-bit shared IR: 18.8 KiB/block, max **0.1400 MH/s** at 90x2;
  this is 69% faster than baseline. Another topology, 64x3, gave 0.1227.
- Alternate grouped opcode dispatch was slower (0.0730 MH/s at 24x8).
- Per-thread local VM register array was slower (0.0690 MH/s at 24x8).
- Earlier unrelated fixed-ProgramSet AOT SM75 CUDA ceiling: 0.707 MH/s,
  excluding compilation. New static interpreter remains below it.

Do not infer a 5.8 MH/s target from the author's 5060 Ti result on this 2060.
Future experiments: profile dynamic opcode branching, register access and
scratch dependency cost; investigate optimized micro-op formats and different
L2-aware launch shapes. Changes must retain exact independent oracle checks.
Benchmark raw data and rejected alternatives are stored under the associated
Asus benchmark directory. Production HashWX APIs remain unchanged.
