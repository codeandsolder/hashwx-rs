# HashX v1 + HashWX 1.0 on AWS EC2 F1

**Status: fully simulated execution/scan engines and F1 OCL BAR0 adapter, not yet an AFI or a physical F1 benchmark.**

## Implemented

The experiment contains two independent single-lane engines targeting the Xilinx XCVU9P in an AWS f1.2xlarge:

- **HashWX 1.0:** 256 generated 16-bit operations, nine 64-bit registers, all 13 opcodes, SipHash setup/finalization, 16 KiB scratchpad + 8-byte sentinel, unaligned loads, snapshots, and branch budget.
- **HashX v1:** 512 generated 64-bit encoded instructions, all 11 opcodes, SipHash setup and 256-bit digest, unsigned/signed high-half multiply, one-shot branching. A single time-multiplexed 16×16 multiplier accumulates 128-bit products in 16 cycles.
- Both have bounded nonce-range scanners with first-hit early exit, independent configuration/status/result registers, and independent seed-specific program upload.
- The Rust SDK host runner verifies the full hardware digest against the authoritative Rust reference before accepting any returned nonce.

The design is **not yet performance competitive**: one lane per algorithm, OCL AXI-Lite scalar configuration, no DMA batch transport or parallel nonce queue. Generic UltraScale+ area mapping is available below; clock rate and timings remain unknown until Vivado placement and routing.

## Test

From this directory, run:

    ./scripts/test.sh

The script uses Rust 1.99 via rustup, managed sccache, Icarus Verilog, Verilator, and the pinned AWS F1 HDK example for a source-level generator regression. Compiled output is stored under /home/jan/builds/hashwx-f1-20261115, not in this repository. Fixtures live in ignored tests/fixtures.

- HashWX 16 generated programs × 3 nonces: **48 exact 64-bit hashes**.
- HashX 36 generated programs (including variable-length seeds) × 3 nonces: **108 exact 256-bit hashes**.
- Rust encoders: 128 independent generated programs per algorithm checked for lossless packing.
- Both scanners: target/exclusive comparison, no-hit, zero count and first-hit conditions.
- Both MMIO banks: program load, start, status, attempts, winner hash.
- Dual-engine BAR0 model: HashX and HashWX run concurrently with independent, correct outputs.

With one shared HashWX 64x64 multiplier and a single scratchpad BRAM read port, simulation costs on tested programs are **15,628 clocks per HashWX candidate** (previously 13,836) and **3,790–3,909 clocks per HashX candidate**. These are **not real FPGA throughput numbers**.

## Synthesized resource footprint (Yosys, UltraScale+)

Run ./scripts/synth.sh from this directory after installing Yosys and ABC. All
temporary netlists, logs and ABC scratch files live under the owned build
directory. These are *unplaced* technology mapping counts, not Vivado area or
timing closure.

| Engine | DSP48E2 | RAMB36E2 | Estimated logic cells |
|---|---:|---:|---:|
| HashWX, original baseline (two multipliers/two scratch read ports) | 20 | 9 | 4,504 |
| HashWX, shared multiplier + single scratch port | 10 | 5 | 4,287 |
| HashX, time-multiplexed 16x16 multiplier | 1 | 1 | 5,006 |
| Combined two-engine BAR0 wrapper + scanner/control | 11 | 6 | 10,230 |

Resource counts are from Yosys 0.66, synth_xilinx -family xcup, with IO pads
and auto clock buffers disabled. The memory optimization costs one additional
cycle per affected read. F1 XCVU9P-specific Vivado timing and proper
DSP/BRAM placement are still required before predicting any hashes/second.

## F1 HDK and custom logic

Use AWS **F1 HDK tag v1.4.25**, not the latest F2-specific branch.

Generate a separate CL by running:

    uv run --no-project python scripts/install-f1-cl.py \
        --example /home/jan/references/aws-fpga-f1-v1.4.25/hdk/cl/examples/cl_hello_world \
        --dest /home/jan/builds/hashwx-f1-20261115/cl_hashwx_hashx

The generator preserves the original AWS example source/licenses, OCL interface, port list, tie-offs and build scripts, adds seven RTL modules and connects the exact same synthesizable dual-engine
BAR0 wrapper used by the testbench, and extends encrypt.tcl. The original HDK checkout is never modified. It refuses to overwrite the destination.

**No Vivado synthesis or AWS AFI has been performed.** On an appropriately licensed F1 Vivado/HDK machine, the generated CL is intended as CL_DIR for the normal AWS design-checkpoint/AFI flow. The shell-level integration still requires a real Vivado test.

## OCL BAR0 register map

Offsets are byte addresses; each access is aligned 32-bit little-endian.

| Meaning | HashWX address | HashX address |
|---|---|---|
| Algorithm magic | 0x600 = 0x48585731 | 0x1000 = 0x48585831 |
| Start (write bit 0) | 0x604 | 0x1004 |
| First nonce, low/high | 0x608 / 0x60c | 0x1008 / 0x100c |
| Exclusive hash target, low/high | 0x610 / 0x614 | 0x1010 / 0x1014 |
| Nonce count (u32) | 0x618 | 0x1018 |
| Status busy/done/found (bits 0,1,2) | 0x61c | 0x101c |
| Attempt count | 0x620 | 0x1020 |
| Winner nonce, low/high | 0x624 / 0x628 | 0x1024 / 0x1028 |
| Winner digest | 0x62c / 0x630 (64 bits) | 0x102c..0x1048 (8×32-bit, 256 bits) |
| Instruction program | 0x800..0x9fc (128×32-bit) | 0x4000..0x4ffc (1024×32-bit) |
| Seed-derived key words | 0xa00..0xa0c (4×32-bit) | 0x5000..0x501c (8×32-bit) |

HashWX instructions: 16 bits (opcode [15:12], dst [11:9], src [8:6], imm [5:0]); RMCG has implicit register 8.
HashX instructions: 64 bits (opcode [63:60], dst [59:57], src [56:54], shift [53:48], signed immediate/branch mask [31:0]); least-significant 32-bit word loaded first.

Configuration writes are only accepted when idle; writes while busy are ignored, not AXI-rejected. The original AWS example 0x500/0x504 registers remain in place.

## Rust host

On an AWS F1 host with the SDK installed and native libfpga_mgmt available:

    cargo +1.99.0 build --release --manifest-path host/Cargo.toml \
        --features aws-f1-sdk --bins

    hashwx-f1-solve <seed_hex_64> <start_nonce> <nonce_count> <exclusive_target> [slot]
    hashx-f1-solve <seed_hex_any_even_length> <start_nonce> <nonce_count> <exclusive_target> [slot]

Arguments accept decimal or 0x-prefixed hex. Host runners check expected magic, upload packed program and keys, initiate bounded scan, check returned nonce range and threshold, and compare the complete hardware digest to the portable Rust implementation. Both runners pass Rust 1.99 all-feature compile checks; AWS SDK linking/execution remains untested.

## Required next work

1. Run full XCVU9P Vivado synthesis, placement and routing to verify exact
BRAM/DSP placement, critical paths, timing closure and power. Yosys UltraScale+
mapping already passes; actual Vivado is not installed.
2. Replace single-lane MMIO execution with many pipelined lanes, batched descriptor dispatch, and DMA result queues.
3. Run F1 HDK full-shell compilation and generate/load an AFI on AWS.
4. Validate actual F1 host-device latency, concurrent work, cost per hash and watts per hash against optimized CPU/GPU baselines.
5. Extend the differential corpus after memory banking, multiplier pipelining and multi-lane changes.

Source: https://github.com/codeandsolder/hashwx-rs
F1 HDK: https://github.com/aws/aws-fpga/tree/v1.4.25
HashWX upstream: https://github.com/tevador/hashwx
