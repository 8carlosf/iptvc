#!/bin/sh
# Build pkg/iptvc.wasm (`mise run wasm`).
set -e
cd "$(dirname "$0")"
cargo build --release --lib --target wasm32-unknown-unknown
mkdir -p pkg && cp target/wasm32-unknown-unknown/release/iptvc_core.wasm pkg/iptvc.wasm
# The features rustc's wasm32 target emits by default.
command -v wasm-opt >/dev/null && wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
  --enable-mutable-globals --enable-multivalue --enable-reference-types pkg/iptvc.wasm -o pkg/iptvc.wasm
ls -l pkg/iptvc.wasm | awk '{print "iptvc.wasm", $5, "bytes"}'
