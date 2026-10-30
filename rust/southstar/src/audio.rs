//! Southstar — the southstar-audio media helper a cargo install builds.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

fn main() {
    southstar_audio_helper::run();
}
