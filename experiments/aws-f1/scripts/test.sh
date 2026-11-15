#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
out=/home/jan/builds/hashwx-f1-20261115
mkdir -p "$out"
export RUSTC_WRAPPER=sccache
export CARGO_TARGET_DIR="$out/cargo"
cargo +1.99.0 run --bin hashwx-f1-fixtures --manifest-path host/Cargo.toml -- tests/fixtures
cargo +1.99.0 test --manifest-path host/Cargo.toml
/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_hashwx -o "$out/hashwx_sim" rtl/hashwx_core.sv rtl/hashwx_scan.sv tests/tb_hashwx.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/hashwx_sim"

/home/jan/tools/iverilog-install/bin/iverilog -g2012 -s tb_hashwx_mmio -o "$out/hashwx_mmio_sim" rtl/hashwx_core.sv rtl/hashwx_scan.sv rtl/hashwx_mmio.sv tests/tb_hashwx_mmio.sv
/home/jan/tools/iverilog-install/bin/vvp "$out/hashwx_mmio_sim"
