#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
src_dir="$PWD/rtl"
wx_lanes=4
hx_lanes=4
mul_partials=4
direct_mul=1
pipe_mul=0
if [ "$#" -ge 3 ]; then mul_partials=$3; fi
if [ "$#" -ge 4 ]; then direct_mul=$4; fi
if [ "$#" -ge 5 ]; then pipe_mul=$5; fi
case "$direct_mul" in 0|1) ;; *) echo "direct_mul must be 0 or 1" >&2; exit 1 ;; esac
case "$pipe_mul" in 0|1) ;; *) echo "pipe_mul must be 0 or 1" >&2; exit 1 ;; esac
if [ "$pipe_mul" = 1 ] && [ "$direct_mul" != 1 ]; then
    echo "pipe_mul requires direct_mul=1" >&2; exit 1
fi
case "$mul_partials" in 1|2|4|8|16) ;; *) echo "MUL_PARTIALS must be 1,2,4,8,16" >&2; exit 1 ;; esac
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
        stem="hash_f1_dual-$wx_lanes-wx-$hx_lanes-hx-$mul_partials-mul-$direct_mul-direct-$pipe_mul-pipe"
        config="chparam -set WX_LANES $wx_lanes -set HX_LANES $hx_lanes -set HX_MUL_PARTIALS $mul_partials -set HX_DIRECT_MUL $direct_mul -set HX_PIPE_MUL $pipe_mul hash_f1_dual;"
    else
        stem="$block"
        config=""
        if [ "$block" = hashx_core ]; then
            stem="hashx_core-$mul_partials-mul-$direct_mul-direct-$pipe_mul-pipe"
            config="chparam -set MUL_PARTIALS $mul_partials -set DIRECT_MUL $direct_mul -set PIPE_MUL $pipe_mul hashx_core;"
        fi
    fi
    echo "Synthesizing $stem for UltraScale+..."
    yosys -Q -T -l "$out/$stem-xcup.log" \
        -p "read_verilog -sv $sources; $config synth_xilinx -family xcup -top $block -noiopad -noclkbuf; stat -tech xilinx" \
        > "$out/$stem-stdout.log" 2>&1
    rg 'DSP48E2|RAMB36E2|Estimated number of LCs' "$out/$stem-xcup.log" | tail -3
done
echo 'Note: These are unplaced Yosys UltraScale+ mapping estimates, not Vivado timing or an AFI.'
