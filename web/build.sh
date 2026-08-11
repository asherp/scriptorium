#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Builds the wasm module the playground loads, into web/pkg/.
#
# Uses wasm-pack when it is on PATH and falls back to plain cargo plus
# wasm-bindgen otherwise, so a bare checkout with nothing but a Rust toolchain
# still gets there. Pass --serve to start a static server on the way out.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

serve=0
port=8080
for arg in "$@"; do
  case "$arg" in
    --serve) serve=1 ;;
    --port=*) port="${arg#--port=}" ;;
    -h|--help)
      echo "usage: web/build.sh [--serve] [--port=N]"
      exit 0
      ;;
    *) echo "build.sh: unknown option $arg" >&2; exit 2 ;;
  esac
done

if command -v wasm-pack >/dev/null 2>&1; then
  wasm-pack build --release --target web --out-dir web/pkg \
    --no-default-features --features wasm
else
  echo "build.sh: no wasm-pack, falling back to cargo + wasm-bindgen"
  rustup target add wasm32-unknown-unknown >/dev/null

  cargo build --release --target wasm32-unknown-unknown \
    --no-default-features --features wasm

  # The CLI has to match the wasm-bindgen the crate was just built against,
  # which the freshly written lockfile is the authority on.
  want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/[",]/,"",$3); print $3; exit}' Cargo.lock)
  have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
  if [ "$have" != "$want" ]; then
    echo "build.sh: installing wasm-bindgen-cli $want (have: ${have:-none})"
    cargo install wasm-bindgen-cli --version "$want" --locked
  fi

  wasm-bindgen --target web --out-dir web/pkg \
    target/wasm32-unknown-unknown/release/scriptorium.wasm
fi

echo "build.sh: wrote $root/web/pkg"

if [ "$serve" = 1 ]; then
  echo "build.sh: serving http://localhost:$port/ (ctrl-c to stop)"
  exec python3 -m http.server "$port" --directory web
fi
