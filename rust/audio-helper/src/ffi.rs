//! Southstar — the C libraries the audio helper calls: SDL2, minimp3, libav*, vorbisfile, opusfile and the OS.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "libav")]
pub mod av;
pub mod clock;
pub mod mp3;
#[cfg(feature = "opusfile")]
pub mod opus;
#[cfg(target_os = "linux")]
pub mod os;
pub mod sdl;
#[cfg(feature = "vorbisfile")]
pub mod vorbis;
