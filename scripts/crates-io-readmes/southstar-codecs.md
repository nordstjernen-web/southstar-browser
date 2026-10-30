# southstar-codecs

The media decoders of [Southstar](https://github.com/nordstjernen-web/southstar-browser),
a web browser written in Rust.

| Module | What it decodes |
| --- | --- |
| `plmpeg` | MPEG-1 program streams: the demuxer, the MPEG-1 video decoder and the MP2 audio decoder. A pure-Rust port of [pl_mpeg](https://github.com/phoboslab/pl_mpeg) (MIT). |
| `minimp3` | MPEG-1/2/2.5 Layer I, II and III audio to float PCM, frame by frame. A pure-Rust port of [minimp3](https://github.com/lieff/minimp3) (CC0). |
| `av` | FFmpeg's `libavformat`/`libavcodec`/`libavutil`/`libswscale`/`libswresample`, over hand-written declarations. |

`plmpeg` and `minimp3` are memory-safe Rust with no C dependency. They are
why a `<video>` with an `.mpg` source and an `<audio>` with an `.mp3` play
in Southstar with no codec library installed.

`av` declares only the FFmpeg calls the browser makes, with no bindgen. Its
build script reads the struct offsets and enum values from the FFmpeg
headers actually installed, through
[southstar-build](https://crates.io/crates/southstar-build), so one source
tree works against FFmpeg 6.0 through 8.x. Without FFmpeg, `av::available()`
returns `false` and WebM playback is simply off.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`; the ports keep their
upstream notices.
