# iptvc

A browser-only IPTV player (PWA) for Xtream Codes servers: live TV, movies, series and catch-up.
Svelte 5 + Vite, a Rust core compiled to wasm, and mediabunny for demuxing and muxing.

```sh
mise run dev     # http://localhost:5173 — sign in with your server URL, username, password
mise run test    # Rust tests + svelte-check
mise run build   # app/dist, a static PWA
```

## How playback works

| Source | Path |
|---|---|
| Movie/episode the browser plays as-is (MP4 + H.264/AAC) | `<video src>` directly; needs no CORS |
| Live TV, catch-up (MPEG-TS) and files with AC-3/DTS/MP2 audio | **remux**: mediabunny demuxes, video packets are copied untouched into fragmented MP4, audio is decoded in wasm and re-encoded to AAC, and the result goes to `<video>` via Media Source Extensions |

Copying the video, rather than decoding it with WebCodecs, matters: Chrome's WebCodecs refuses interlaced H.264,
while its `<video>` path decodes MBAFF 1080i fine. Mid-stream joins drop open-GOP leading pictures.

**PAFF 1080i** (each field its own picture, common on broadcast HD channels) decodes in no Chrome path. Those
streams go to FFmpeg's H.264 decoder compiled to wasm (libav.js, `mise run decoder`, ~1.2 MB, loaded only when
needed; about 6× real time for 1080i on one core). Its frames are drawn on a WebGL canvas, while MSE carries
only the audio and `<video>` stays the clock.

The Rust core (`core/`, string ABI, no bindgen) does:
- **MP2 audio decoding** (symphonia), which DVB channels carry and browsers can't decode;
- **H.264 SPS/slice-header parsing**, to spot PAFF streams and interlaced ones;
- **catalog search** over the stream lists (held in wasm; accent- and superscript-folded: "news fhd" finds "News ᶠᴴᴰ");
- **EPG decoding** (base64 titles, catch-up flags).

AC-3/E-AC-3 and DTS come from mediabunny's FFmpeg-in-wasm extensions. Interlaced streams are deinterlaced
(linear blend) by a WebGL mirror of the video, switched on automatically from the stream's SPS.

## Server requirements

- **https**: an https page can't fetch `http://` streams, so use the server's https address
  (e.g. behind Traefik, which also brings HTTP/2), or serve the app over plain http.
- **CORS on movie and series files**, for the AC-3/DTS/MKV path (the API, live and catch-up already send it).
  With Traefik, a headers middleware on the IPTV router:
  ```yaml
  http:
    middlewares:
      iptv-cors:
        headers:
          accessControlAllowOriginList: ["*"]
          accessControlExposeHeaders: ["Content-Length", "Content-Range", "Accept-Ranges"]
  ```
  Without it those files still play through `<video>`, but with no sound when the audio is AC-3/DTS (the
  player says so). For development only: `IPTVC_PROXY=http://host:port mise run dev`, then sign in with server `/xt`.

## Setup

`mise install`, then `mise run decoder` once (downloads and builds FFmpeg with emsdk; output goes to
`app/public/libav/`, gitignored), then `mise run dev`.
