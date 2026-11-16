# HashX v1 + HashWX 1.0 on AWS EC2 F1

**Status: fully simulated execution/scan engines and F1 OCL BAR0 adapter, not yet an AFI or a physical F1 benchmark.**

## Implemented

The experiment contains two independent, separately parameterized multi-lane engines targeting the Xilinx XCVU9P in an AWS f1.2xlarge:

- **HashWX 1.0:** 256 generated 16-bit operations, nine 64-bit registers, all 13 opcodes, SipHash setup/finalization, 16 KiB scratchpad + 8-byte sentinel, unaligned loads, snapshots, and branch budget.
- **HashX v1:** 512 generated 64-bit encoded instructions, all 11 opcodes, SipHash setup and 256-bit digest, unsigned/signed high-half multiply, one-shot branching. A single time-multiplexed 16×16 multiplier accumulates 128-bit products in 16 cycles.
- Both use a shared parameterized, wave-scheduled search controller, with independently configurable lane counts (four of each by default), broadcast seed-program upload, and independent configuration/status/result registers. Program storage is replicated in each lane so asynchronous execution can fetch different PCs.
- Search work is issued in contiguous nonce waves, and results commit in nonce order: even when one lane finishes earlier, the lowest qualifying nonce in the earliest completed wave wins. A partial final wave only starts the required lanes. The attempt counter records **actual computed hashes** (including other lanes in the winning wave), not the winning nonce's 1-based index.
- Each algorithm has independent **four-entry** command and completion FIFOs; 32-bit tags preserve descriptor identity while the host posts more ranges. Results stay readable until explicitly acknowledged, with per-descriptor attempt counts.
- The Rust SDK host runner streams arbitrary u64-length searches as 16,384-nonce tagged jobs, respects FIFO backpressure, drains queued work after a winner, and verifies every winning digest against the CPU implementation.

The design is **not yet a production throughput solver**: four lanes per algorithm by default, BAR0 descriptor/result FIFOs and a connected FPGA-initiated PCIM host-memory ring engine. The PCIM RTL is simulation-tested, but real pinned host memory and Vivado shell validation remain. Generic UltraScale+ area mapping is available below; clock rate and timing remain unknown until Vivado placement and routing.

## Test

From this directory, run:

    ./scripts/test.sh

The script uses Rust 1.99 via rustup, managed sccache, Icarus Verilog, Verilator, and the pinned AWS F1 HDK example for a source-level generator regression. Compiled output is stored under /home/jan/builds/hashwx-f1-20261115, not in this repository. Fixtures live in ignored tests/fixtures.

- HashWX 16 generated programs × 3 nonces: **48 exact 64-bit hashes**.
- HashX 36 generated programs (including variable-length seeds) × 3 nonces: **108 exact 256-bit hashes**.
- Rust encoders: 128 independent generated programs per algorithm checked for lossless packing.
- Both scanners: target/exclusive comparison, no-hit, zero count, later-wave first hit, multi-hit nonce ordering, partial final waves, 64-bit nonce wraparound and program reload.
- Parallel scheduler oracle testing for **1, 2, 3, 4, 5, 7 and 8 lanes** per algorithm. Each setting tests both hashes against 64 sequential nonce reference results, plus wraparound oracle results.
- Both MMIO banks: program upload, four-entry job enqueue, completion retention until explicit pop, tags, attempts, winner hash, invalid program-write rejection, and readable FIFO status.
- Non-power-of-two depth-3 FIFO simulation checks ordering, command backpressure and completion backpressure without executing the cryptographic cores.
- Full dual-engine MMIO simulation: six concurrently queued jobs (three per algorithm) with independent results and complete CPU-oracle digests. Includes no-hit, early-hit and zero-count jobs.
- New F1 PCIM host-memory model: 24 queued HashWX/HashX completions through 512-bit AXI4 transfers, delayed handshakes, correct reference digests, ring wraparound, full output-ring backpressure and resume on acknowledgment, and a malformed-descriptor fault.
- Eight aggregate Rust PCIM/FIFO host tests cover descriptor encoding, full 256-bit completion decoding, physical ring validation, producer/ack constraints and previously established queue checks.
- Rust host queue transport is abstracted behind a small MMIO bus trait, with mock-device unit tests independent of the AWS SDK. Coverage includes arbitrary u64-length counts, nonce overflow, multi-batch submission, draining in-flight work after a hit, tag mismatch detection, preexisting protocol errors, and zero-work requests.

With one shared HashWX 64x64 multiplier and a single scratchpad BRAM read port, simulation costs on tested programs are **15,628 clocks per HashWX candidate** (previously 13,836) and **3,791–3,910 clocks per HashX candidate**. These are **not real FPGA throughput numbers**.

## Synthesized resource footprint (Yosys, UltraScale+)

Run ./scripts/synth.sh from this directory after installing Yosys and ABC. All
temporary netlists, logs and ABC scratch files live under the owned build
directory. These are *unplaced* technology mapping counts, not Vivado area or
timing closure.

| Engine | DSP48E2 | RAMB36E2 | Estimated logic cells |
|---|---:|---:|---:|
| HashWX, original baseline (two multipliers/two scratch read ports) | 20 | 9 | 4,504 |
| HashWX, shared multiplier + single scratch port | 10 | 5 | 4,287 |
| HashX, time-multiplexed 16x16 multiplier and shared SipRound | 1 | 1 | 4,820 |
| Combined scalar (one lane each) BAR0 wrapper + scanner/control | 11 | 6 | 10,230 |
| Combined 4 HashWX + 4 HashX lanes with wave scheduler (before FIFO) | 44 | 24 | 37,387 |
| Combined 4+4 lanes, four-deep command/result FIFO per algorithm | 44 | 24 | 38,325 |
| Combined 4+4 lanes, FIFOs and connected PCIM host-memory DMA | **44** | **24** | **39,283** |

Resource counts are from Yosys 0.66, synth_xilinx -family xcup, with IO pads
and auto clock buffers disabled. The memory optimization costs one additional
cycle per affected read. HashX additionally reuses its init SipRound for both
final digest rounds by inserting one extra finalization cycle and keeping
the intermediate digest in the existing state register. This reduces HashX
from 5,006 to 4,820 estimated logic cells without changing the 256-bit hash. F1 XCVU9P-specific Vivado timing and proper
DSP/BRAM placement are still required before predicting any hashes/second.

## Parallel scheduling and simulated throughput

Both banks default to four independent lanes. A seed-specific instruction set
is uploaded once over BAR0 and broadcast into each lane's private program RAM.
For custom builds, edit the hash_f1_dual WX_LANES and HX_LANES parameters, or
measure alternative configurations using:

    ./scripts/synth.sh 4 4

This uses a compile-time lane count; the host can discover it through read-only
BAR0 registers 0x634 (HashWX) and 0x104c (HashX). Zero and partial waves do
not activate idle lanes. Nonce arithmetic wraps modulo 2^64, matching the CPU.

Icarus Verilog tests for **16 consecutive no-hit nonces** show these clock
counts (includes wave scheduling; no known FPGA clock frequency):

| Lanes per algorithm | HashWX cycles | HashX cycles |
|---|---:|---:|
| 1 | 250,080 | 62,354 |
| 2 | 125,040 | 31,296 |
| 3 | 93,780 | 23,472 |
| 4 | 62,520 | 15,648 |
| 5 | 62,520 | 15,648 |
| 7 | 46,890 | 11,736 |
| 8 | 31,260 | 7,824 |

This confirms the scheduler's near-linear **simulated-cycle** scaling;
without placement/timing, do not interpret it as an actual FPGA hash-rate or
cost-per-hash measurement. The BAR0 register banks buffer four commands and four completed jobs per algorithm, while each scanner still executes one command at a time. A found result is selected only after its current nonce wave finishes.

## F1 HDK and custom logic

Use AWS **F1 HDK tag v1.4.25**, not the latest F2-specific branch.

Generate a separate CL by running:

    uv run --no-project python scripts/install-f1-cl.py \
        --example /home/jan/references/aws-fpga-f1-v1.4.25/hdk/cl/examples/cl_hello_world \
        --dest /home/jan/builds/hashwx-f1-20261115/cl_hashwx_hashx

The generator preserves the original AWS example source/licenses, OCL interface, port list, tie-offs and build scripts, adds ten RTL modules and connects the exact same synthesizable dual-engine BAR0 wrapper used by the testbench, and extends encrypt.tcl. The original HDK checkout is never modified. It refuses to overwrite the destination.

**No Vivado synthesis or AWS AFI has been performed.** On an appropriately licensed F1 Vivado/HDK machine, the generated CL is intended as CL_DIR for the normal AWS design-checkpoint/AFI flow. The shell-level integration still requires a real Vivado test.

## F1 v2 BAR0 queued-command ABI

This intentionally replaces the old single-scan protocol without legacy
opcode support. Identification markers are now **HXW2** and **HXX2**.

Each bank has one staging descriptor: first nonce, exclusive target, u32
count and u32 tag. Writing bit 0 to enqueue snapshots those fields into a
four-entry FIFO. Jobs execute in submission order; completions are buffered
in a separate four-entry FIFO until explicitly popped. Program uploads are
only permitted when command and completion FIFOs and the scanner are idle.
Rejected writes set sticky error flags until explicitly cleared.

All addresses are 32-bit little-endian BAR0 byte offsets.

| Meaning | HashWX | HashX |
|---|---|---|
| Algorithm marker | 0x600 = 0x48585732 | 0x1000 = 0x48585832 |
| Enqueue staged job (write bit 0) | 0x604 | 0x1004 |
| Staged nonce, low/high | 0x608 / 0x60c | 0x1008 / 0x100c |
| Staged exclusive target, low/high | 0x610 / 0x614 | 0x1010 / 0x1014 |
| Staged count (u32) | 0x618 | 0x1018 |
| Status flags | 0x61c | 0x101c |
| Head completion attempts | 0x620 | 0x1020 |
| Head completion winning nonce, low/high | 0x624 / 0x628 | 0x1024 / 0x1028 |
| Head completion digest | 0x62c..0x630 (64 bits) | 0x102c..0x1048 (256 bits) |
| Physical lane count (read-only) | 0x634 | 0x104c |
| Head completion tag | 0x638 | 0x1050 |
| Pop head completion (write bit 0) | 0x63c | 0x1054 |
| Pending command/completion counts | 0x640 | 0x1058 |
| Staged 32-bit command tag | 0x644 | 0x105c |
| Queue depth (read-only) | 0x648 | 0x1060 |
| Sticky error register / write-1-to-clear | 0x64c | 0x1064 |
| Instruction program | 0x800..0x9fc (128×32-bit) | 0x4000..0x4ffc (1024×32-bit) |
| Seed-derived key words | 0xa00..0xa0c (4×32-bit) | 0x5000..0x501c (8×32-bit) |

Status flags: bit 0 = not idle (including unread results); bit 1 = a
completion is available; bit 2 = head result found; bit 3 = command FIFO
can accept a job; bit 4 = sticky enqueue overflow; bit 5 = sticky program
write rejection. FIFO count register packs completion count in upper 16
bits and waiting command count in lower 16 bits; the executing job is not
counted as pending. Error register bits 0 and 1 mirror overflow and
rejected program writes; write 1 to acknowledge the corresponding bit.

The head completion is stable until popped, including the full HashX
256-bit digest. Enqueueing when full is rejected and flagged; the host
must check bit 3 before enqueuing. Full completion FIFO backpressure
stalls subsequent dispatch rather than losing results. Zero-count
commands still generate zero-attempt completions. Nonce arithmetic
wraps modulo 2^64. AWS example 0x500/0x504 registers remain untouched.

HashWX instructions: 16 bits (opcode [15:12], dst [11:9],
src [8:6], imm [5:0]); RMCG uses implicit register 8.
HashX instructions: 64 bits (opcode [63:60], dst [59:57],
src [56:54], shift [53:48], signed immediate / branch mask [31:0]);
least-significant 32-bit word loaded first.

## Rust host

On an AWS F1 host with the SDK installed and native libfpga_mgmt available:

    cargo +1.99.0 build --release --manifest-path host/Cargo.toml \
        --features aws-f1-sdk --bins

    hashwx-f1-solve <seed_hex_64> <start_nonce> <nonce_count> <exclusive_target> [slot]
    hashx-f1-solve <seed_hex_any_even_length> <start_nonce> <nonce_count> <exclusive_target> [slot]

Arguments accept decimal or 0x-prefixed hex. Nonce count is u64;
the host partitions it into 16,384-nonce tagged commands, posts several
ahead where the queue allows, and reads completion entries in submission
order. It verifies all winning candidates and full digests with the CPU,
and pops each result. After a hit it stops submitting new work but drains
already submitted jobs, leaving the FPGA idle for the next program load.

This is **MMIO-batched**, not PCIe DMA. It removes per-scan host
synchronization, but true high-volume streaming still needs DMA into
device-resident descriptor/result rings via the AWS shell. Both Rust host
runners pass Rust 1.99 all-feature check and Clippy; AWS SDK native
linking/execution remain untested.

## Required next work

1. Run full XCVU9P Vivado synthesis, placement and routing to verify exact
BRAM/DSP placement, critical paths, timing closure and power. Yosys UltraScale+
mapping already passes; actual Vivado is not installed.
2. Increase/optimize lane counts beyond four per algorithm, profile
critical paths, provision real pinned physical host-memory rings for the
new PCIM DMA engine, and validate the F1 shell interface in Vivado.
The current DMA AXI sequencer permits one outstanding transfer at a time.
3. Run F1 HDK full-shell compilation and generate/load an AFI on AWS.
4. Validate actual F1 host-device latency, concurrent work, cost per hash and watts per hash against optimized CPU/GPU baselines.
5. Extend the differential corpus after memory banking, multiplier pipelining and multi-lane changes.

Source: https://github.com/codeandsolder/hashwx-rs
F1 HDK: https://github.com/aws/aws-fpga/tree/v1.4.25
HashWX upstream: https://github.com/tevador/hashwx

## F1 host-memory PCIM DMA (RTL and mock-AXI tested)

The generated CL connects the **F1 v1.4.25 PCIM 512-bit AXI4** master to
the actual scanner queues, replacing the unused-PCIM tie-offs. BAR0
0x2000..0x2030 configures two separate host-memory rings. Each command
and result is exactly one **64-byte aligned AXI beat**. The rings each
hold 16 slots and use monotonic wrapping 32-bit counters.

### Descriptor and result layout, little-endian

| Byte offset | Input descriptor | Output completion |
|---|---|---|
| 0x00..0x03 | u32 tag | u32 tag |
| 0x04..0x07 | u32 algorithm (0=HashWX,1=HashX) | u32 algorithm |
| 0x08..0x0b | u32 nonce count | u32 attempts |
| 0x0c..0x0f | zero | u32 found (0/1) |
| 0x10..0x17 | u64 first nonce | u64 winner nonce, or zero |
| 0x18..0x1f | u64 exclusive target | zero |
| 0x20..0x3f | zero | full 256-bit digest (HashWX upper 192 bits zero) |

### DMA BAR0 controls

| Address | Semantics |
|---|---|
| 0x2000 | Arm (write 1), or stop when idle/drained (write 0); read active |
| 0x2004 | Read marker 0x5043494d ('PCIM') |
| 0x2008/0x200c | Descriptor ring physical base, low/high |
| 0x2010/0x2014 | Completion ring physical base, low/high |
| 0x2018 | Host u32 command producer doorbell |
| 0x201c | Host u32 completion acknowledgment |
| 0x2020 | FPGA u32 descriptor-consumed counter |
| 0x2024 | FPGA u32 result-produced counter |
| 0x2028 | Internal DMA state |
| 0x202c | Sticky DMA fault code (reset required after fatal error) |
| 0x2030 | Read ring size, 16 entries |

The PCIM reader issues 512-bit single-beat AR/R transfers (AxLEN=0,
AxSIZE=6), validates each descriptor and sends it to the appropriate
HashWX or HashX queue when ready. The result mover arbitrates between
both queues, writes one full completion with AW/W/B and **only pops the
result after a successful B response**. It blocks when the host-memory
result ring is full; host acknowledgments release slots. Nonzero AXI
responses, malformed descriptors and ring-capacity violations cause a
sticky fault rather than lost records. Physical address ranges must
be nonzero, 64-byte aligned, nonoverlapping and free of address wrap.

The DMA engine is exclusively active after arm, so BAR0 enqueue/pop and
program writes are disabled while DMA is running. The older MMIO v2
queue is still usable while DMA is disabled. This is a host-memory
PCIM bus master, **not** the shell's XDMA transfer-to-FPGA-DDR facility.

**Host provisioning boundary:** AWS's pinned F1 shell specification
requires *physical addresses*, not userspace virtual pointers. Portable
Rust host code in host/src/pcim.rs provides the exact record encoding,
decode and checked counter operations with release/acquire CPU fences.
It **does not allocate or pin real DMA buffers** or translate a userspace
virtual pointer into a physical bus address. A real F1 host still needs
a supported physically contiguous, pinned, coherent buffer allocation
path (accounting for its IOMMU setup). No Vivado compilation, AFI or
physical host/FPGA run has been performed. The first RTL sequencer allows
one outstanding PCIe transaction; pipelining needs physical measurements.

## Linux F1 host: coherent PCI DMA memory and Rust runner

The host now has a bound PCI driver at `host/linux/hashwx_f1.c`, separate from
the AWS SDK BAR0-only runner. It binds an AWS AppPF device ID (`1d0f:f000`
through `f003`; other AFI IDs can be added using the PCI driver `new_id`)
and calls `dma_alloc_coherent()` on the **actual PCI requester**. This
returns the DMA *bus address* for the device, accounting for IOMMU
translation if present; it is not a guessed CPU physical address.

The driver enables 64-bit DMA, maps only the needed 24 KiB of BAR0,
and exposes a 0600-mode `/dev/hashwx-f1-<PCI-BDF>` device with checked
ioctls for BAR0 reads/writes and coherent DMA allocation info. An
`.mmap()` shares one 4096-byte DMA allocation. The 16 descriptor slots
start at offset 0 and the 16 completion slots at offset 1024, each slot
64 bytes. The driver restricts opening to one process, and during unbind
waits for the user file and mappings to close before freeing DMA memory.

Build on the **F1 host with matching kernel headers** (objects stay outside
the source tree):

```bash
./scripts/build-f1-driver.sh
# Then, as root on the F1 host after loading the appropriate AFI:
# insmod /home/<user>/builds/hashwx-f1/linux-<kernel>/src/hashwx_f1.ko
```

Use `KERNEL_RELEASE`, `KERNEL_HEADERS` or `F1_BUILD_ROOT` if needed.
The device has permission mode 0600, so root or an explicit administrator
permission change is required.

The new `host/src/linux.rs` implements the same `QueueIo` abstraction with
kernel ioctls and volatile coherent-memory accesses. The runnable binary
`hashwx-f1-pcim` loads either seed program, posts tagged 16,384-candidate
jobs to the host-memory DMA ring, handles result-ring backpressure,
verifies **every matching complete digest** with the CPU oracle, drains
outstanding jobs and disarms the PCIM engine when idle:

```bash
cd host
cargo +1.99.0 run --release --bin hashwx-f1-pcim -- \
  /dev/hashwx-f1-0000:00:00.0 wx SEED_HEX_64 0 1000000 0xffffffffffffffff
```

Replace the sample BDF and seed with actual values. Full device execution
remains untested: there is no F1 AppPF on Acer. The module was compiled
against the installed 7.3.3 kernel headers but the running Acer kernel
is 7.2.8, so it was **not loaded**. An actual F1 host, an AFI and full
Vivado shell validation are still needed.
