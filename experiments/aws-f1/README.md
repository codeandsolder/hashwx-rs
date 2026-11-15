# HashWX 1.0 FPGA prototype for AWS EC2 F1

Status: **working RTL simulation / F1 OCL integration source, not yet a synthesized AFI**.

This experiment supplies a HashWX 1.0 program interpreter, bounded nonce-range scanner,
F1 BAR0 MMIO endpoint, AWS F1 HDK CL generator, and Rust SDK host runner.
It reuses the generated, immutable ProgramSet and oracle from this repository.
**HashX v1 is not yet implemented in FPGA RTL**; it has a different 512-instruction ISA,
64-bit signed/unsigned high-half multiplies, and one-shot branches.

## Reproduce local correctness

On Acer, run:

    ./scripts/test.sh

The test harness uses Rust 1.99 and managed sccache, compiles/runs the
Rust reference program encoder and two Icarus Verilog testbenches.
Default build outputs go to /home/jan/builds/hashwx-f1-20261115,
and fixture outputs under ignored tests/fixtures.

Currently verified:
- 16 independent generated HashWX programs, three nonces per program = 48 exact
  64-bit FPGA-RTL vs portable-Rust matches.
- 128 additional generated programs checked for lossless compact IR encoding.
- Bounded scan correctness: no-hit, empty range, equality boundary, first-hit exit.
- MMIO program upload and start/status/winner register readback.
- One reference program requires 13,836 simulated clock cycles per nonce;
  **no FPGA frequency, throughput, resource use, or PCIe latency is established**.

## F1-specific integration

Use **aws/aws-fpga tag v1.4.25** (the F1 shell), rather than the current AWS
F2 HDK. From the repository's experiments/aws-f1 directory:

    uv run --no-project python scripts/install-f1-cl.py \
      --example /home/jan/references/aws-fpga-f1-v1.4.25/hdk/cl/examples/cl_hello_world \
      --dest /home/jan/builds/hashwx-f1-20261115/cl_hashwx

The generated CL preserves the AWS example licensing/header, CL port list,
AXI-Lite OCL register slice, resets, tie-offs, IP and Vivado scripts.
The generator injects the HashWX OCL endpoint in cl_hello_world.sv,
adds the three custom SV modules, and extends encrypt.tcl to copy them.
The example's build workflow may then be used with CL_DIR pointing to that
generated cl_hashwx location on an F1-compatible HDK/Vivado environment.

AWS F1 uses the Xilinx Virtex UltraScale+ XCVU9P. Bitstream/AFI generation and
PCIe execution **have not been run here** (no licensed Vivado/physical F1 in
this development environment). This wrapper therefore remains an untested
Vivado integration even though its MMIO endpoint passes direct RTL simulation.

### OCL MMIO map (AppPF BAR0, aligned u32 offsets)

| Offset | Direction | Meaning |
|---|---|---|
| 0x600 | R | Magic 0x48585731 ("HXW1") |
| 0x604 | W | Start command: bit 0 |
| 0x608..0x60c | RW | Starting 64-bit nonce (low, high) |
| 0x610..0x614 | RW | Exclusive 64-bit hash threshold (low, high) |
| 0x618 | RW | Number of sequential nonces to try (u32) |
| 0x61c | R | Busy bit 0, done bit 1, found bit 2 |
| 0x620 | R | Number of attempted nonces |
| 0x624..0x628 | R | Winning nonce (low, high) |
| 0x62c..0x630 | R | Winning full 64-bit hash (low, high) |
| 0x800..0x9fc | W | 128 u32 words, each encoding two consecutive 16-bit instructions |
| 0xa00..0xa0c | W | k0 and k1 as four little-endian 32-bit words |

Instruction bit layout:
opcode [15:12], destination [11:9], source [8:6], immediate [5:0].
RMCG uses implicit register 8 (the source field is ignored).
Configure programs **only while idle**; the current interface does not
implement an AXI error response for writes attempted while busy.
No double-buffering or multi-lane replication has been implemented.

### Rust host SDK runner

On an F1 instance with AWS SDK userspace installed, build with Rust 1.99:

    cargo +1.99.0 build --release --manifest-path host/Cargo.toml \
      --features aws-f1-sdk --bin hashwx-f1-solve

That binary dynamically uses the AWS SDK symbols from libfpga_mgmt, which must
be installed and linkable on the F1 host. Invoke it with:

    hashwx-f1-solve <64-hex-digit-seed> <start-nonce> <nonce-count> <exclusive-target> [slot]

Each uint64 argument accepts decimal or 0x-prefixed hexadecimal.
The runner validates the HXW1 marker, uploads an authoritative generated
ProgramSet, starts a bounded search, polls status, and **CPU-verifies** any
FPGA-reported candidate against hashwx-rs. Compilation on Acer used
cargo check (not link or runtime verification).

## Architecture and outstanding work

Each lane has 256 16-bit instructions, nine 64-bit VM registers, 16 KiB
of scratch plus a sentinel word, and SipHash-style initialization/finalization.
The scratch is held in a 64-bit word memory; two synchronous reads reconstruct
byte-unaligned 64-bit source values. The register-phase snapshots and both
phase-specific branch rules are implemented.

Before treating this as a competitive FPGA solver:
1. Run Vivado synthesis, utilization and timing against F1's VU9P and inspect
   the multiplier/BRAM mapping. Share and pipeline the 64x64 arithmetic.
2. Replace 32-bit MMIO upload and one-nonce-at-a-time control with batched
   DMA and a multi-lane compute engine; measure real clock and PCIe overhead.
3. Add the separate HashX v1 ISA and register initialization/digest RTL,
   with exact Tor/reference differential vectors and seed rejection handling.
4. Build the AFI through the F1 HDK and test it on f1.2xlarge.
5. Measure performance per dollar against the existing optimized CPU solver.

Upstream sources:
- https://github.com/aws/aws-fpga/tree/v1.4.25
- https://github.com/tevador/hashwx
- https://github.com/codeandsolder/hashwx-rs

All original AWS example code remains external to this experiment until the
CL installer copies it into a build-only location.
