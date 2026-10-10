//! Southstar — the per-site camera permission and the YUYV frame conversion behind webcam capture.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

const ALLOWED: i32 = 1;
const DENIED: i32 = 2;
const ASKED: i32 = 3;

#[derive(Default)]
struct Permissions {
    decisions: HashMap<Vec<u8>, i32>,
    pending: Option<Vec<u8>>,
}

static PERMISSIONS: LazyLock<Mutex<Permissions>> = LazyLock::new(Mutex::default);

fn permissions() -> MutexGuard<'static, Permissions> {
    PERMISSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn take_pending_origin() -> Option<Vec<u8>> {
    permissions().pending.take()
}

pub fn set_decision(origin: &[u8], allow: bool) {
    if origin.is_empty() {
        return;
    }
    permissions()
        .decisions
        .insert(origin.to_vec(), if allow { ALLOWED } else { DENIED });
    if allow {
        southstar_config::enable_camera();
    }
    let mut state = permissions();
    if state.pending.as_deref() == Some(origin) {
        state.pending = None;
    }
}

fn allowed_by_environment() -> bool {
    std::env::var_os("NS_CAMERA_ALLOW")
        .is_some_and(|value| value.as_encoded_bytes().first() == Some(&b'1'))
}

pub fn permission(url: Option<&[u8]>, origin: Option<Vec<u8>>) -> i32 {
    let origin = origin
        .filter(|origin| !origin.is_empty())
        .unwrap_or_else(|| match url {
            Some(url) if !url.is_empty() => url.to_vec(),
            _ => b"this page".to_vec(),
        });
    let mut state = permissions();
    if let Some(&decision) = state.decisions.get(&origin) {
        return match decision {
            ALLOWED => 1,
            DENIED => 0,
            _ => -1,
        };
    }
    if allowed_by_environment() {
        state.decisions.insert(origin, ALLOWED);
        return 1;
    }
    state.decisions.insert(origin.clone(), ASKED);
    state.pending = Some(origin);
    -1
}

fn clamp(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

pub fn yuyv_to_bgra(
    src: &[u8],
    width: usize,
    height: usize,
    src_stride: usize,
    dst: &mut [u8],
    dst_stride: usize,
) {
    let pairs = width / 2;
    for row in 0..height {
        let Some(line) = src.get(row * src_stride..row * src_stride + pairs * 4) else {
            break;
        };
        let out = &mut dst[row * dst_stride..row * dst_stride + pairs * 8];
        let (pixels, _) = out.as_chunks_mut::<8>();
        let (samples, _) = line.as_chunks::<4>();
        for (pixel, &[y0, u, y1, v]) in pixels.iter_mut().zip(samples) {
            let (u, v) = (i32::from(u) - 128, i32::from(v) - 128);
            for (k, luma) in [y0, y1].into_iter().enumerate() {
                let y = i32::from(luma) - 16;
                let r = (298 * y + 409 * v + 128) >> 8;
                let g = (298 * y - 100 * u - 208 * v + 128) >> 8;
                let b = (298 * y + 516 * u + 128) >> 8;
                pixel[k * 4..k * 4 + 4].copy_from_slice(&[clamp(b), clamp(g), clamp(r), 255]);
            }
        }
    }
}
