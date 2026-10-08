//! Southstar — the toolkit-independent offscreen GL context WebGL renders into, with the attribute lists each backend asks for.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub mod egl {
    pub const NONE: i32 = 0x3038;
    pub const TRUE: i32 = 1;
    pub const EXTENSIONS: i32 = 0x3055;
    pub const PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
    pub const OPENGL_ES_API: u32 = 0x30A0;
    const SURFACE_TYPE: i32 = 0x3033;
    const PBUFFER_BIT: i32 = 0x0001;
    const RENDERABLE_TYPE: i32 = 0x3040;
    const OPENGL_ES2_BIT: i32 = 0x0004;
    const RED_SIZE: i32 = 0x3024;
    const GREEN_SIZE: i32 = 0x3023;
    const BLUE_SIZE: i32 = 0x3022;
    const ALPHA_SIZE: i32 = 0x3021;
    const CONTEXT_MAJOR_VERSION: i32 = 0x3098;
    const CONTEXT_OPENGL_ROBUST_ACCESS_EXT: i32 = 0x30BF;
    const CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT: i32 = 0x3138;
    const LOSE_CONTEXT_ON_RESET_EXT: i32 = 0x31BF;
    const WIDTH: i32 = 0x3057;
    const HEIGHT: i32 = 0x3056;

    pub const PBUFFER_CONFIG: [i32; 13] = [
        SURFACE_TYPE,
        PBUFFER_BIT,
        RENDERABLE_TYPE,
        OPENGL_ES2_BIT,
        RED_SIZE,
        8,
        GREEN_SIZE,
        8,
        BLUE_SIZE,
        8,
        ALPHA_SIZE,
        8,
        NONE,
    ];

    pub const ANY_CONFIG: [i32; 11] = [
        RENDERABLE_TYPE,
        OPENGL_ES2_BIT,
        RED_SIZE,
        8,
        GREEN_SIZE,
        8,
        BLUE_SIZE,
        8,
        ALPHA_SIZE,
        8,
        NONE,
    ];

    pub const PBUFFER_SIZE: [i32; 5] = [WIDTH, 1, HEIGHT, 1, NONE];

    pub fn robust_context(major: i32) -> [i32; 7] {
        [
            CONTEXT_MAJOR_VERSION,
            major,
            CONTEXT_OPENGL_ROBUST_ACCESS_EXT,
            TRUE,
            CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT,
            LOSE_CONTEXT_ON_RESET_EXT,
            NONE,
        ]
    }

    pub fn plain_context(major: i32) -> [i32; 3] {
        [CONTEXT_MAJOR_VERSION, major, NONE]
    }
}

pub fn has_extension(list: &[u8], name: &str) -> bool {
    list.windows(name.len())
        .any(|window| window == name.as_bytes())
}
