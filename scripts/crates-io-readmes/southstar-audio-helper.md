# southstar-audio-helper

The `southstar-audio` process of
[Southstar](https://github.com/nordstjernen-web/southstar-browser): the
program that plays the sound of `<audio>` and `<video>` elements.

The browser's renderer runs in a sandbox with no audio device. When a page
plays media, the shell starts this helper and sends it line commands: `open`,
`play`, `pause`, `seek`, `stop`, `loop` and `volume`. The helper downloads the
stream, decodes it, mixes and resamples every playing stream itself, and
outputs through SDL2's audio device (WASAPI, CoreAudio or ALSA). It publishes
its playback clock in shared memory, which the renderer keeps video frames in
sync with.

It decodes:
- MPEG-1/MP2 and MP3, with Southstar's pure-Rust ports of pl_mpeg and minimp3
- WebM, Opus and Vorbis, through FFmpeg when the `libav` feature is on
- Ogg Vorbis and Opus natively, through libvorbisfile and libopusfile, with the `vorbisfile`/`opusfile` features

On Linux it confines itself with the same Landlock and seccomp profile as the
renderer once its device and network stack are up. Attacker-controlled codec
bytes are therefore decoded inside a sandbox.

`cargo install southstar` builds it as the `southstar-audio` executable;
this crate exposes `run()`.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
