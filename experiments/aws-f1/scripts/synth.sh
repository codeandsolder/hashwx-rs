#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
src_dir="$PWD/rtl"
out=/home/jan/builds/hashwx-f1-20261115/synthesis
mkdir -p "$out"
command -v yosys >/dev/null || { echo 'Install Yosys (e.g. sudo pacman -S yosys abc)' >&2; exit 1; }
# All ABC scratch output (including history) belongs in the build directory.
cd "$out"
for block in hashx_core hashwx_core hash_f1_dual; do
    case "$block" in
        hashx_core) sources="$src_dir/hashx_core.sv" ;;
        hashwx_core) sources="$src_dir/hashwx_core.sv" ;;
        hash_f1_dual)
            sources="$src_dir/hashwx_core.sv $src_dir/hashwx_scan.sv $src_dir/hashwx_mmio.sv $src_dir/hashx_core.sv $src_dir/hashx_scan.sv $src_dir/hashx_mmio.sv $src_dir/hash_f1_dual.sv" ;;
    esac
    echo "Synthesizing $block for UltraScale+..."
    yosys -Q -T -l "$out/$block-xcup.log" \
        -p "read_verilog -sv $sources; synth_xilinx -family xcup -top $block -noiopad -noclkbuf; stat -tech xilinx" \
        > "$out/$block-stdout.log" 2>&1
    rg 'DSP48E2|RAMB36E2|Estimated number of LCs' "$out/$block-xcup.log" | tail -3
done
echo 'Note: These are unplaced Yosys UltraScale+ mapping estimates, not Vivado timing or an AFI.'
