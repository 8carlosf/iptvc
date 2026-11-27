# iptvc

A browser-only IPTV player (PWA), live at **https://8carlosf.github.io/iptvc/**. Sources:
- **M3U playlist** (the default; prefilled with [iptv-org](https://github.com/iptv-org/iptv)'s public one), with an
  optional account (sent as `username`/`password` query parameters) and XMLTV guide: live channels, filtered by
  `group-title` and by the country code in `tvg-id`, guides matched by `tvg-id`.
- **Xtream Codes**: live TV, movies, series and catch-up.

Public playlists get channel checks: free ones from the playlist itself (needs a Referer or a TV app's User-Agent,
http-only, region-locked, part-time), and a **Check** button for lists of up to 300 channels that asks each stream
whether it answers (6 at a time, one per host, results kept for an hour). Providers that count connections never
get bulk checks.

The sign-in form remembers each source's addresses separately; passwords are never stored.
Svelte 5 + Vite, a Rust core compiled to wasm, and mediabunny for demuxing and muxing.

> **Disclaimer:** iptvc is only a player. It ships no channels, hosts no streams and has no ties to any provider.
> You bring your own playlists and accounts and are responsible for having the right to watch what they carry.
> The iptv-org playlist it suggests is a third-party list of publicly available streams.

**http vs https:** the hosted copy (GitHub Pages) is https-only, and browsers block an https page from loading
`http://` playlists, providers or streams. Use https sources there, or run it locally over http.

```sh
mise run dev     # http://localhost:5173
mise run test    # Rust tests, svelte-check, unit tests
mise run build   # app/dist, a static PWA
```

## How playback works

| Source | Path |
|---|---|
| Movie/episode the browser plays as-is (MP4 + H.264/AAC), and HLS where the browser has it (Chrome, Safari) | `<video src>` directly; needs no CORS |
| DASH (`.mpd`) | dash.js, loaded only then; DRM-encrypted channels can't play and say so |
| Live TV, catch-up (MPEG-TS) and files with AC-3/DTS/MP2 audio | **remux**: mediabunny demuxes, video packets are copied untouched into fragmented MP4, audio is decoded in wasm and re-encoded to AAC, and the result goes to `<video>` via Media Source Extensions |

Copying the video, rather than decoding it with WebCodecs, matters: Chrome's WebCodecs refuses interlaced H.264,
while its `<video>` path decodes MBAFF 1080i fine. Mid-stream joins drop open-GOP leading pictures.

**PAFF 1080i** (each field its own picture, common on broadcast HD channels) decodes in no Chrome path. Those
streams go to FFmpeg's H.264 decoder compiled to wasm (libav.js, prebuilt in `app/public/libav/`, ~1.2 MB, loaded only when
needed; about 6× real time for 1080i on one core). Its frames are drawn on a WebGL canvas, while MSE carries
only the audio and `<video>` stays the clock.

The Rust core (`core/`, string ABI, no bindgen) does:
- **MP2 audio decoding** (symphonia), which DVB channels carry and browsers can't decode;
- **H.264 SPS/slice-header parsing**, to spot PAFF streams and interlaced ones;
- **M3U and XMLTV parsing** (quick-xml), and **catalog search** over the stream lists (held in wasm; accent- and superscript-folded: "news fhd" finds "News ᶠᴴᴰ");
- **EPG decoding** (base64 titles, catch-up flags).

AC-3/E-AC-3 and DTS come from mediabunny's FFmpeg-in-wasm extensions. Interlaced streams are deinterlaced
(linear blend) by a WebGL mirror of the video, switched on automatically from the stream's SPS.

## Server requirements

- **M3U and XMLTV URLs must allow cross-origin reads** (`Access-Control-Allow-Origin`), since the page fetches them.
  Channels themselves play through `<video>` and need none, unless they go through the remux path (HLS the browser
  can't decode itself, e.g. HEVC, falls back to it). Channels that need a custom User-Agent or Referer can't play in
  a browser.

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

`mise install`, then `mise run dev`. The FFmpeg decoder is committed prebuilt in `app/public/libav/`;
`mise run decoder` rebuilds it (downloads FFmpeg via libav.js and compiles it with emsdk), only needed to change it.

Pushes to `main` deploy to GitHub Pages (`.github/workflows/pages.yml`).

## License

MIT (`LICENSE`), except `app/public/libav/` (libav.js + FFmpeg, LGPL 2.1+; sources and rebuild steps in `LICENSE`).
