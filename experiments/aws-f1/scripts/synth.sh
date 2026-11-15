#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
src_dir="$PWD/rtl"
wx_lanes=4
hx_lanes=4
if [ "$#" -ge 1 ]; then wx_lanes=$1; fi
if [ "$#" -ge 2 ]; then hx_lanes=$2; fi
case "$wx_lanes:$hx_lanes" in
    *[!0-9:]*|0:*|*:0) echo "lane counts must be positive integers" >&2; exit 1 ;;
esac
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
            sources="$src_dir/hashwx_core.sv $src_dir/hashwx_scan.sv $src_dir/hashwx_mmio.sv $src_dir/hashx_core.sv $src_dir/hashx_scan.sv $src_dir/hashx_mmio.sv $src_dir/hash_parallel_scan.sv $src_dir/hash_job_queue.sv $src_dir/hash_pcim_dma.sv $src_dir/hash_f1_dual.sv" ;;
    esac
    if [ "$block" = hash_f1_dual ]; then
        stem="hash_f1_dual-$wx_lanes-wx-$hx_lanes-hx"
        config="chparam -set WX_LANES $wx_lanes -set HX_LANES $hx_lanes hash_f1_dual;"
    else
        stem="$block"
        config=""
    fi
    echo "Synthesizing $stem for UltraScale+..."
    yosys -Q -T -l "$out/$stem-xcup.log" \
        -p "read_verilog -sv $sources; $config synth_xilinx -family xcup -top $block -noiopad -noclkbuf; stat -tech xilinx" \
        > "$out/$stem-stdout.log" 2>&1
    rg 'DSP48E2|RAMB36E2|Estimated number of LCs' "$out/$stem-xcup.log" | tail -3
done
echo 'Note: These are unplaced Yosys UltraScale+ mapping estimates, not Vivado timing or an AFI.'
