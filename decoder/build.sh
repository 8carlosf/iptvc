#!/bin/sh
# Build app/public/libav/ with a libav.js (FFmpeg → wasm) variant holding only the video decoders broadcast TV needs and
# Chrome lacks: H.264 field pictures (PAFF 1080i) and MPEG-2 video. `mise run decoder`; needs emsdk (mise.toml).
set -e
cd "$(dirname "$0")"
[ -d libav.js ] || git clone -q --depth 1 -b v6.11.9.0.2 https://github.com/Yahweasel/libav.js
cd libav.js
(cd configs && node mkconfig.js iptvc '["avcodec","decoder-h264","parser-h264","decoder-mpeg2video","parser-mpegvideo"]')
make -j8 build-iptvc
# Served as-is (the wrapper starts its worker from its own URL), so they go to public/, not through the bundler.
out=../../app/public/libav
mkdir -p $out && cp dist/libav-iptvc.mjs dist/libav-*-iptvc.wasm.mjs dist/libav-*-iptvc.wasm.wasm $out/
ls -l $out
