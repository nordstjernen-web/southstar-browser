//! Southstar — drawImage, getImageData and putImageData on the 2D context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::draw::state;
use crate::ffi::cairo::{Context, Surface};

type Result = core::result::Result<Value, Value>;

const FILTER_NEAREST: i32 = 3;
const FILTER_BILINEAR: i32 = 4;
const MAX_SIDE: i64 = 32767;

fn arg(scope: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    args.get(i)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

fn int(scope: &mut Scope<'_>, v: Option<&Value>) -> i32 {
    v.and_then(|v| scope.to_int32(v).ok()).unwrap_or(0)
}

pub(crate) fn draw_image(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(source) = args.first().filter(|v| v.is_object()) else {
        return Err(scope.type_error(
            "Failed to execute 'drawImage' on 'CanvasRenderingContext2D': argument 1 is not a \
             valid image source.",
        ));
    };
    if args.len() < 3 {
        return Ok(Value::undefined());
    }
    let Some(src) = crate::ffi::drawimage_source(scope, source) else {
        return Ok(Value::undefined());
    };
    let (total_w, total_h) = src.size;
    if total_w <= 0 || total_h <= 0 {
        return Ok(Value::undefined());
    }
    let (tw, th) = (f64::from(total_w), f64::from(total_h));
    let [sx, sy, sw, sh, dx, dy, dw, dh] = if args.len() >= 9 {
        let mut v = [0.0; 8];
        for (i, slot) in v.iter_mut().enumerate() {
            *slot = arg(scope, args, i + 1);
        }
        v
    } else if args.len() >= 5 {
        let dx = arg(scope, args, 1);
        let dy = arg(scope, args, 2);
        let dw = arg(scope, args, 3);
        let dh = arg(scope, args, 4);
        [0.0, 0.0, tw, th, dx, dy, dw, dh]
    } else {
        let dx = arg(scope, args, 1);
        let dy = arg(scope, args, 2);
        [0.0, 0.0, tw, th, dx, dy, tw, th]
    };
    if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
        return Ok(Value::undefined());
    }
    let ga = crate::style::global_alpha(scope, this);
    let smooth = crate::style::image_smoothing(scope, this);
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    let Some(cr) = (unsafe { Context::from_raw(st.cr) }) else {
        return Ok(Value::undefined());
    };
    cr.save();
    crate::style::apply_composite(scope, this, cr);
    cr.translate(dx, dy);
    cr.scale(dw / sw, dh / sh);
    cr.translate(-sx, -sy);
    cr.rectangle(sx, sy, sw, sh);
    cr.clip();
    cr.set_source_surface(&src.surface, 0.0, 0.0);
    cr.set_source_filter(if smooth {
        FILTER_BILINEAR
    } else {
        FILTER_NEAREST
    });
    if ga < 1.0 - 1e-6 {
        cr.paint_with_alpha(ga);
    } else {
        cr.paint();
    }
    cr.restore();
    if !src.origin_clean {
        st.origin_clean = 0;
    }
    drop(src);
    crate::ffi::mark_mutated(scope);
    Ok(Value::undefined())
}

fn unpremultiply(b: u8, g: u8, r: u8, a: u8) -> [u8; 4] {
    match a {
        0 => [0, 0, 0, 0],
        255 => [r, g, b, 255],
        _ => {
            let a32 = u32::from(a);
            let un = |c: u8| ((u32::from(c) * 255 + a32 / 2) / a32) as u8;
            [un(r), un(g), un(b), a]
        }
    }
}

fn premultiply(r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
    let scale = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
    match a {
        0 => [0, 0, 0, 0],
        255 => [b, g, r, a],
        _ => [scale(b), scale(g), scale(r), a],
    }
}

pub(crate) fn get_image_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let sx = int(scope, args.first());
    let sy = int(scope, args.get(1));
    let sw = int(scope, args.get(2));
    let sh = int(scope, args.get(3));
    let st = state(scope, this);
    if st.as_ref().is_some_and(|st| st.origin_clean == 0) {
        return Err(crate::api::throw_dom(
            scope,
            "SecurityError",
            "The canvas has been tainted by cross-origin data.",
        ));
    }
    let (mut ox, mut oy, mut rw, mut rh) =
        (i64::from(sx), i64::from(sy), i64::from(sw), i64::from(sh));
    if rw < 0 {
        ox += rw;
        rw = -rw;
    }
    if rh < 0 {
        oy += rh;
        rh = -rh;
    }
    if rw == 0 || rh == 0 {
        let message = if rw == 0 {
            "The source width is 0."
        } else {
            "The source height is 0."
        };
        return Err(crate::api::throw_dom(scope, "IndexSizeError", message));
    }
    if rw > MAX_SIDE || rh > MAX_SIDE {
        return Err(scope.range_error("getImageData region too large"));
    }
    let (dw, dh) = (rw as usize, rh as usize);
    let mut out = Vec::new();
    if out.try_reserve_exact(dw * dh * 4).is_err() {
        return Err(scope.range_error("getImageData allocation failed"));
    }
    out.resize(dw * dh * 4, 0);
    let surface = st.and_then(|st| unsafe { Surface::from_borrowed(st.surf) });
    if let Some(surface) = surface {
        surface.read_pixels(|data, stride, (cw, ch)| {
            for y in 0..dh {
                let src_y = oy + y as i64;
                for x in 0..dw {
                    let src_x = ox + x as i64;
                    if src_x < 0 || src_y < 0 || src_x >= i64::from(cw) || src_y >= i64::from(ch) {
                        continue;
                    }
                    let p = src_y as usize * stride + src_x as usize * 4;
                    let px = unpremultiply(data[p], data[p + 1], data[p + 2], data[p + 3]);
                    out[(y * dw + x) * 4..(y * dw + x) * 4 + 4].copy_from_slice(&px);
                }
            }
        });
    }
    crate::ffi::new_imagedata_from(scope, this, (dw as i32, dh as i32), &out)
}

pub(crate) fn put_image_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let Some(image) = args.first().filter(|v| v.is_object() && args.len() >= 3) else {
        return Ok(Value::undefined());
    };
    let wv = scope
        .get(image, "width")
        .unwrap_or_else(|_| Value::undefined());
    let hv = scope
        .get(image, "height")
        .unwrap_or_else(|_| Value::undefined());
    let data = scope
        .get(image, "data")
        .unwrap_or_else(|_| Value::undefined());
    let iw = scope.to_int32(&wv).unwrap_or(0);
    let ih = scope.to_int32(&hv).unwrap_or(0);
    if iw <= 0 || ih <= 0 || i64::from(iw) > MAX_SIDE || i64::from(ih) > MAX_SIDE {
        return Ok(Value::undefined());
    }
    let dx = int(scope, args.get(1));
    let dy = int(scope, args.get(2));
    let (mut rx, mut ry, mut rw, mut rh) = (0i64, 0i64, i64::from(iw), i64::from(ih));
    if args.len() >= 7 {
        rx = i64::from(int(scope, args.get(3)));
        ry = i64::from(int(scope, args.get(4)));
        rw = i64::from(int(scope, args.get(5)));
        rh = i64::from(int(scope, args.get(6)));
    }
    if rw < 0 {
        rx += rw;
        rw = -rw;
    }
    if rh < 0 {
        ry += rh;
        rh = -rh;
    }
    if rx < 0 {
        rw += rx;
        rx = 0;
    }
    if ry < 0 {
        rh += ry;
        ry = 0;
    }
    if rx + rw > i64::from(iw) {
        rw = i64::from(iw) - rx;
    }
    if ry + rh > i64::from(ih) {
        rh = i64::from(ih) - ry;
    }
    if rw <= 0 || rh <= 0 {
        return Ok(Value::undefined());
    }
    let Some(st) = state(scope, this) else {
        return Ok(Value::undefined());
    };
    let Some(surface) = (unsafe { Surface::from_borrowed(st.surf) }) else {
        return Ok(Value::undefined());
    };
    let needed = iw as usize * ih as usize * 4;
    let pixels = scope
        .with_typed_array(&data, |t| {
            (t.bytes.len() >= needed).then(|| t.bytes[..needed].to_vec())
        })
        .flatten();
    let Some(pixels) = pixels else {
        return Ok(Value::undefined());
    };
    let (cw, ch) = surface.size();
    if surface.read_pixels(|_, _, _| ()).is_none() {
        return Ok(Value::undefined());
    }
    surface.write_pixels(|dst, stride| {
        for y in 0..rh {
            let dst_y = i64::from(dy) + y;
            if dst_y < 0 || dst_y >= i64::from(ch) {
                continue;
            }
            for x in 0..rw {
                let dst_x = i64::from(dx) + x;
                if dst_x < 0 || dst_x >= i64::from(cw) {
                    continue;
                }
                let s = (((ry + y) * i64::from(iw) + rx + x) * 4) as usize;
                let px = premultiply(pixels[s], pixels[s + 1], pixels[s + 2], pixels[s + 3]);
                let p = dst_y as usize * stride + dst_x as usize * 4;
                dst[p..p + 4].copy_from_slice(&px);
            }
        }
    });
    crate::ffi::mark_mutated(scope);
    Ok(Value::undefined())
}
