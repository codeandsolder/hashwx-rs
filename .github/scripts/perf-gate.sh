#!/usr/bin/env bash
set -euo pipefail

readonly HASHWX_CANONICAL_REV=74b567a31276a4c5d7cb232a5b7639467f49f961
readonly HASHX_CANONICAL_REV=08babdf4f41b0b8991d1fa94914c7c6902de0cb6
readonly MIN_RATIO=0.95
readonly SWEEPS=3
readonly CHUNKS=5
readonly WORK_ROOT="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/hashwx-canonical-perf"

rm -rf "$WORK_ROOT"
mkdir -p "$WORK_ROOT"

fetch_repo() {
  local url=$1 rev=$2 dst=$3
  git init -q "$dst"
  git -C "$dst" remote add origin "$url"
  git -C "$dst" fetch -q --depth 1 origin "$rev"
  git -C "$dst" checkout -q --detach FETCH_HEAD
}

fetch_repo https://github.com/tevador/hashwx.git "$HASHWX_CANONICAL_REV" "$WORK_ROOT/hashwx"
fetch_repo https://github.com/tevador/hashx.git "$HASHX_CANONICAL_REV" "$WORK_ROOT/hashx"

cmake_args=(-DCMAKE_BUILD_TYPE=Release)
if [[ -n "${SCCACHE_BIN:-}" ]]; then
  cmake_args+=("-DCMAKE_C_COMPILER_LAUNCHER=$SCCACHE_BIN")
fi
cmake -S "$WORK_ROOT/hashwx" -B "$WORK_ROOT/hashwx/build" "${cmake_args[@]}" >/dev/null
cmake --build "$WORK_ROOT/hashwx/build" --target hashwx-bench --parallel >/dev/null
cmake -S "$WORK_ROOT/hashx" -B "$WORK_ROOT/hashx/build" "${cmake_args[@]}" >/dev/null
cmake --build "$WORK_ROOT/hashx/build" --target hashx-bench --parallel >/dev/null

cargo +1.99.0 build --locked --release --all-features --example bench_canonical
readonly OURS="${CARGO_TARGET_DIR:-target}/release/examples/bench_canonical"
readonly CANONICAL_HASHWX="$WORK_ROOT/hashwx/build/hashwx-bench"
readonly CANONICAL_HASHX="$WORK_ROOT/hashx/build/hashx-bench"

declare -a sweep_ratios

workload() {
  local mode=$1
  if [[ "$mode" == hashwx ]]; then
    printf '%s %s\n' 3000 463
  else
    printf '%s %s\n' 500 65536
  fi
}

sample_canonical() {
  local mode=$1 start=$2 seeds=$3 nonces=$4 output hashes rate
  if [[ "$mode" == hashwx ]]; then
    output="$($CANONICAL_HASHWX --threads 1 --start "$start" --seeds "$seeds" --nonces "$nonces")"
  else
    output="$($CANONICAL_HASHX --threads 1 --start "$start" --seeds "$seeds" --nonces "$nonces")"
  fi
  hashes="$(awk '/Total hashes:/ { print $3; exit }' <<<"$output")"
  rate="$(awk '/hashes\/sec\./ { print $1; exit }' <<<"$output")"
  awk -v hashes="$hashes" -v rate="$rate" 'BEGIN { printf "%s %.12f\n", hashes, hashes / rate }'
}

sample_ours() {
  local mode=$1 start=$2 seeds=$3 nonces=$4 output hashes rate
  output="$("$OURS" "$mode" --start "$start" --seeds "$seeds" --nonces "$nonces")"
  hashes="$(awk -F'total_hashes=' '{ split($2, a, " "); print a[1]; exit }' <<<"$output")"
  rate="$(awk -F'hashes_per_sec=' '{ split($2, a, " "); print a[1]; exit }' <<<"$output")"
  awk -v hashes="$hashes" -v rate="$rate" 'BEGIN { printf "%s %.12f\n", hashes, hashes / rate }'
}

median3() {
  printf '%s\n' "$@" | sort -n | awk 'NR == 2 { print; exit }'
}

run_sweep() {
  local mode=$1 sweep=$2 total_seeds=$3 nonces=$4
  local base chunk remainder chunk_seeds start canonical_sample ours_sample
  local chunk_hashes chunk_seconds canonical_rate ours_rate ratio
  local canonical_hashes=0 ours_hashes=0 canonical_seconds=0 ours_seconds=0

  base=$((total_seeds / CHUNKS))
  remainder=$((total_seeds % CHUNKS))

  for ((chunk = 0; chunk < CHUNKS; chunk++)); do
    chunk_seeds=$base
    if (( chunk < remainder )); then
      chunk_seeds=$((chunk_seeds + 1))
    fi
    start=$((chunk * base + (chunk < remainder ? chunk : remainder)))

    # Alternate each adjacent pair, and reverse the starting order each sweep,
    # so hosted-runner scheduling/thermal drift is shared by both implementations.
    if (( (chunk + sweep) % 2 == 0 )); then
      canonical_sample="$(sample_canonical "$mode" "$start" "$chunk_seeds" "$nonces")"
      ours_sample="$(sample_ours "$mode" "$start" "$chunk_seeds" "$nonces")"
    else
      ours_sample="$(sample_ours "$mode" "$start" "$chunk_seeds" "$nonces")"
      canonical_sample="$(sample_canonical "$mode" "$start" "$chunk_seeds" "$nonces")"
    fi

    read -r chunk_hashes chunk_seconds <<<"$canonical_sample"
    canonical_hashes=$((canonical_hashes + chunk_hashes))
    canonical_seconds="$(awk -v a="$canonical_seconds" -v b="$chunk_seconds" 'BEGIN { printf "%.12f", a + b }')"

    read -r chunk_hashes chunk_seconds <<<"$ours_sample"
    ours_hashes=$((ours_hashes + chunk_hashes))
    ours_seconds="$(awk -v a="$ours_seconds" -v b="$chunk_seconds" 'BEGIN { printf "%.12f", a + b }')"
  done

  canonical_rate="$(awk -v hashes="$canonical_hashes" -v seconds="$canonical_seconds" 'BEGIN { printf "%.6f", hashes / seconds }')"
  ours_rate="$(awk -v hashes="$ours_hashes" -v seconds="$ours_seconds" 'BEGIN { printf "%.6f", hashes / seconds }')"
  ratio="$(awk -v ours="$ours_rate" -v canonical="$canonical_rate" 'BEGIN { printf "%.6f", ours / canonical }')"
  printf '%s sweep=%d canonical=%s ours=%s ratio=%s\n' "$mode" "$((sweep + 1))" "$canonical_rate" "$ours_rate" "$ratio" >&2
  printf '%s\n' "$ratio"
}

run_gate() {
  local mode=$1 total_seeds nonces sweep ratio warm_seeds
  read -r total_seeds nonces <<<"$(workload "$mode")"
  sweep_ratios=()

  # Warm one chunk of each implementation before measured sweeps.
  warm_seeds=$((total_seeds / CHUNKS))
  sample_canonical "$mode" 0 "$warm_seeds" "$nonces" >/dev/null
  sample_ours "$mode" 0 "$warm_seeds" "$nonces" >/dev/null

  for ((sweep = 0; sweep < SWEEPS; sweep++)); do
    sweep_ratios+=("$(run_sweep "$mode" "$sweep" "$total_seeds" "$nonces")")
  done

  ratio="$(median3 "${sweep_ratios[@]}")"
  printf '%s paired-median ratio=%s minimum=%s\n' "$mode" "$ratio" "$MIN_RATIO"
  awk -v ratio="$ratio" -v minimum="$MIN_RATIO" 'BEGIN { exit !(ratio + 0.0 >= minimum + 0.0) }' || {
    echo "$mode performance regression: hashwx-rs is below 95% of canonical throughput" >&2
    return 1
  }
}

run_gate hashwx
run_gate hashx
