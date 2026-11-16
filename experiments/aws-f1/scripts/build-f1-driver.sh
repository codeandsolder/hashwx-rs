#!/usr/bin/env bash
# Keep ALL external kernel module build artifacts under ~/builds.
set -euo pipefail
src="$(cd "$(dirname "$0")/../host/linux" && pwd)"
release="$(uname -r)"
if [[ -n "$(printenv KERNEL_RELEASE || true)" ]]; then release="$KERNEL_RELEASE"; fi
headers="/lib/modules/$release/build"
if [[ -n "$(printenv KERNEL_HEADERS || true)" ]]; then headers="$KERNEL_HEADERS"; fi
out="$HOME/builds/hashwx-f1/linux-$release"
if [[ -n "$(printenv F1_BUILD_ROOT || true)" ]]; then out="$F1_BUILD_ROOT"; fi
if [[ ! -d "$headers" ]]; then
  echo "Kernel headers unavailable: $headers (install matching headers)" >&2
  exit 1
fi
mkdir -p "$out/src"
cp "$src/hashwx_f1.c" "$src/hashwx_f1_uapi.h" "$src/Makefile" "$out/src/"
make -C "$headers" M="$out/src" modules
echo "Module: $out/src/hashwx_f1.ko"
