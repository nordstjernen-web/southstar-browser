//! Southstar — experimental WebGPU (navigator.gpu) over the external wgpu-native library, ported from webgpu.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod context;
mod encode;
mod ffi;
mod gpu;
mod install;
mod objects;
mod parse;
mod pipeline;
mod util;
