#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Same toolchain rule as build-wasm.sh: rustup-managed cargo with the
# wasm32-wasip1 target installed (Homebrew's cargo lacks wasm targets).
(cd core && cargo build --lib --release --target wasm32-wasip1)

mkdir -p sdk/golang/kernel
cp core/target/wasm32-wasip1/release/diff_fusion.wasm sdk/golang/kernel/diff_fusion.wasm
ls -la sdk/golang/kernel/diff_fusion.wasm
