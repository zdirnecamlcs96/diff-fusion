#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
(cd core && cargo build --target wasm32-unknown-unknown --release)

# wasm-bindgen CLI must match the crate version pinned in Cargo.toml
# (`wasm-bindgen = "=0.2.100"`) — a mismatch produces artifacts the runtime
# glue can't load.
EXPECTED_WASM_BINDGEN="0.2.100"
ACTUAL_WASM_BINDGEN="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$ACTUAL_WASM_BINDGEN" != "$EXPECTED_WASM_BINDGEN" ]; then
  echo "error: wasm-bindgen CLI is $ACTUAL_WASM_BINDGEN, expected $EXPECTED_WASM_BINDGEN (pinned in Cargo.toml)" >&2
  exit 1
fi

wasm-bindgen core/target/wasm32-unknown-unknown/release/diff_fusion.wasm \
  --target nodejs --out-dir sdk/typescript/wasm
ls -la sdk/typescript/wasm/
