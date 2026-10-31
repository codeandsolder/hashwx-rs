#!/usr/bin/env bash
set -euo pipefail

readonly HASHWX_CANONICAL_REV=74b567a31276a4c5d7cb232a5b7639467f49f961
readonly HASHX_CANONICAL_REV=08babdf4f41b0b8991d1fa94914c7c6902de0cb6
readonly MIN_RATIO=0.95
readonly ROUNDS=5
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

declare -a canonical_rates ours_rates

rate_canonical() {
  local mode=$1 output
  if [[ "$mode" == hashwx ]]; then
    output="$($CANONICAL_HASHWX --threads 1 --seeds 3000 --nonces 463)"
  else
    output="$($CANONICAL_HASHX --threads 1 --seeds 500 --nonces 65536)"
  fi
  awk '/hashes\/sec\./ { print $1; exit }' <<<"$output"
}

rate_ours() {
  local mode=$1
  "$OURS" "$mode" | awk -F'hashes_per_sec=' '{ split($2, a, " "); print a[1]; exit }'
}

median() {
  local -a values=("$@")
  printf '%s\n' "${values[@]}" | sort -n | awk 'NR == 3 { print; exit }'
}

run_gate() {
  local mode=$1 round canonical ours canonical_median ours_median ratio
  canonical_rates=()
  ours_rates=()

  # Untimed warmup for code/data pages and JIT machinery.
  rate_canonical "$mode" >/dev/null
  rate_ours "$mode" >/dev/null

  for ((round = 0; round < ROUNDS; round++)); do
    if (( round % 2 == 0 )); then
      canonical="$(rate_canonical "$mode")"
      ours="$(rate_ours "$mode")"
    else
      ours="$(rate_ours "$mode")"
      canonical="$(rate_canonical "$mode")"
    fi
    canonical_rates+=("$canonical")
    ours_rates+=("$ours")
    printf '%s round=%d canonical=%s ours=%s ratio=' "$mode" "$((round + 1))" "$canonical" "$ours"
    awk -v ours="$ours" -v canonical="$canonical" 'BEGIN { printf "%.4f\n", ours / canonical }'
  done

  canonical_median="$(median "${canonical_rates[@]}")"
  ours_median="$(median "${ours_rates[@]}")"
  ratio="$(awk -v ours="$ours_median" -v canonical="$canonical_median" 'BEGIN { printf "%.6f", ours / canonical }')"
  printf '%s median canonical=%s ours=%s ratio=%s minimum=%s\n' \
    "$mode" "$canonical_median" "$ours_median" "$ratio" "$MIN_RATIO"
  awk -v ratio="$ratio" -v minimum="$MIN_RATIO" 'BEGIN { exit !(ratio + 0.0 >= minimum + 0.0) }' || {
    echo "$mode performance regression: hashwx-rs is below 95% of canonical throughput" >&2
    return 1
  }
}

run_gate hashwx
run_gate hashx
