#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
out=/home/jan/builds/hashwx-f1-20261115
mkdir -p "$out"
export RUSTC_WRAPPER=sccache
export CARGO_TARGET_DIR="$out/cargo"
cargo +1.99.0 run --bin hashwx-f1-fixtures --manifest-path host/Cargo.toml -- tests/fixtures
cargo +1.99.0 test --manifest-path host/Cargo.toml
/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_hashwx -o "$out/hashwx_sim" rtl/hashwx_core.sv rtl/hash_parallel_scan.sv rtl/hashwx_scan.sv tests/tb_hashwx.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/hashwx_sim"

/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_hashwx_mmio -o "$out/hashwx_mmio_sim" rtl/hashwx_core.sv rtl/hash_parallel_scan.sv rtl/hashwx_scan.sv rtl/hash_job_queue.sv rtl/hashwx_mmio.sv tests/tb_hashwx_mmio.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/hashwx_mmio_sim"
cargo +1.99.0 run --bin hashx-f1-fixtures --manifest-path host/Cargo.toml -- tests/fixtures
/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_hashx -o "$out/hashx_sim" rtl/hashx_core.sv tests/tb_hashx.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/hashx_sim"
/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_hashx_scan -o "$out/hashx_scan_sim" rtl/hashx_core.sv rtl/hash_parallel_scan.sv rtl/hashx_scan.sv tests/tb_hashx_scan.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/hashx_scan_sim"
/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_hashx_mmio -o "$out/hashx_mmio_sim" rtl/hashx_core.sv rtl/hash_parallel_scan.sv rtl/hashx_scan.sv rtl/hash_job_queue.sv rtl/hashx_mmio.sv tests/tb_hashx_mmio.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/hashx_mmio_sim"
/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_f1_dual -o "$out/f1_dual_sim" \
  rtl/hashwx_core.sv rtl/hash_parallel_scan.sv rtl/hashwx_scan.sv rtl/hash_job_queue.sv rtl/hashwx_mmio.sv \
  rtl/hashx_core.sv rtl/hashx_scan.sv rtl/hashx_mmio.sv rtl/hash_pcim_dma.sv rtl/hash_f1_dual.sv tests/tb_f1_dual.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/f1_dual_sim"
# Lint exactly the dual-engine top instantiated by the generated F1 shell.
verilator --lint-only -Wall -Wno-UNUSEDSIGNAL --top-module hash_f1_dual rtl/*.sv
# Validate the actual AWS v1.4.25 template without leaving a generated CL.
uv run --no-project python tests/test_cl_installer.py

# Differential scoreboard over non-power-of-two and boundary lane counts.
for lanes in 1 2 3 4 5 7 8; do
    /home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_parallel         -Ptb_parallel.LANES="$lanes" -o "$out/parallel-$lanes.vvp"         rtl/*.sv tests/tb_parallel.sv
    /home/jan/tools/iverilog-install/bin/vvp "$out/parallel-$lanes.vvp"
done
# Independent non-power-of-two FIFO/backpressure protocol test.
iverilog=/home/jan/tools/iverilog-install/bin/iverilog
vvp=/home/jan/tools/iverilog-install/bin/vvp
"$iverilog" -g2012 -s tb_job_queue -o "$out/queue_sim" rtl/hash_job_queue.sv tests/tb_job_queue.sv
"$vvp" "$out/queue_sim"
"$iverilog" -g2012 -s tb_mmio_queue -o "$out/mmio_queue_sim" rtl/*.sv tests/tb_mmio_queue.sv
"$vvp" "$out/mmio_queue_sim"
# End-to-end host-memory PCIM AXI4 model including actual HashX/HashWX
# programs, ring wrap, output backpressure and malformed descriptor faults.
"$iverilog" -g2012 -s tb_pcim_dma -o "$out/pcim_dma_sim" rtl/*.sv tests/tb_pcim_dma.sv
"$vvp" "$out/pcim_dma_sim"


# Re-run all 108 full-digest oracle cases with each multiplication topology
# so future fixes cannot accidentally be correct only for the default.
for mul in 1 2 4 8 16; do
    "$iverilog" -g2012 -s tb_hashx         -Ptb_hashx.MUL_PARTIALS="$mul" -Ptb_hashx.DIRECT_MUL=0         -o "$out/hashx-mul-$mul.vvp"         rtl/hashx_core.sv tests/tb_hashx.sv
    "$vvp" "$out/hashx-mul-$mul.vvp"
done
"$iverilog" -g2012 -s tb_hashx -Ptb_hashx.DIRECT_MUL=1     -o "$out/hashx-direct.vvp"     rtl/hashx_core.sv tests/tb_hashx.sv
"$vvp" "$out/hashx-direct.vvp"
