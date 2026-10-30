# southstar-video-helper

The `southstar-video` process of
[Southstar](https://github.com/nordstjernen-web/southstar-browser): it
decodes the video of Media Source Extensions streams, the way YouTube and
most streaming sites deliver video.

The renderer writes the growing media stream to the cache directory and
sends this helper `video …` commands. The helper:
- demuxes and decodes the stream with FFmpeg's libav;
- converts each frame to BGRA;
- writes the frames into a ring in shared memory.

The shell composites those frames over the page surface on every frame tick,
so the renderer process never holds decoded video.

**Small and confined.** The helper links only libav and the small sandbox
and download layer, none of the engine. On Linux it confines itself with
Landlock and seccomp.

`cargo install southstar` builds it as the `southstar-video` executable when
the `libav` feature is on, as it is by default. This crate exposes `run()`.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
