//! Southstar — the C ABI of pagination, as declared in src/print.h, over the style, paint and Cairo calls it needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::sync::OnceLock;

use southstar_glib::{FALSE, GArray, GBoolean, g_array_append_vals, g_array_new};
use southstar_layout::{BoxRef, NsBox, Style};

const A4_WIDTH: f64 = 210.0 * 96.0 / 25.4;
const A4_HEIGHT: f64 = 297.0 * 96.0 / 25.4;
const MARGIN: f64 = 0.5 * 96.0;

#[repr(C)]
pub struct PrintSetup {
    width: f64,
    height: f64,
    margin_top: f64,
    margin_right: f64,
    margin_bottom: f64,
    margin_left: f64,
}

#[repr(C)]
pub struct PageRule {
    width: f64,
    height: f64,
    has_size: GBoolean,
    landscape: GBoolean,
    margin: [f64; 4],
    has_margin: [GBoolean; 4],
}

#[derive(Clone, Copy)]
pub enum BreakProp {
    Before,
    After,
    Inside,
}

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_style_keyword(style: *const Style, prop: c_int) -> *const c_char;
    fn ns_paint_css_line_height_px(style: *const Style) -> f64;
    fn ns_paint(cr: *mut c_void, root: *const NsBox, highlight_query: *const c_char);
    fn cairo_save(cr: *mut c_void);
    fn cairo_restore(cr: *mut c_void);
    fn cairo_scale(cr: *mut c_void, sx: f64, sy: f64);
    fn cairo_translate(cr: *mut c_void, tx: f64, ty: f64);
    fn cairo_rectangle(cr: *mut c_void, x: f64, y: f64, width: f64, height: f64);
    fn cairo_clip(cr: *mut c_void);
}

fn break_props() -> &'static [c_int; 3] {
    static PROPS: OnceLock<[c_int; 3]> = OnceLock::new();
    PROPS.get_or_init(|| {
        [c"break-before", c"break-after", c"break-inside"]
            .map(|name| unsafe { ns_css_prop_id(name.as_ptr()) })
    })
}

pub fn break_keyword(b: BoxRef<'_>, prop: BreakProp) -> Option<&CStr> {
    let style = b.style();
    if style.is_null() {
        return None;
    }
    let kw = unsafe { ns_style_keyword(style, break_props()[prop as usize]) };
    (!kw.is_null()).then(|| unsafe { CStr::from_ptr(kw) })
}

pub fn line_height_px(b: BoxRef<'_>) -> f64 {
    let style = b.style();
    if style.is_null() {
        return 0.0;
    }
    unsafe { ns_paint_css_line_height_px(style) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_print_setup_default(setup: *mut PrintSetup) {
    unsafe {
        *setup = PrintSetup {
            width: A4_WIDTH,
            height: A4_HEIGHT,
            margin_top: MARGIN,
            margin_right: MARGIN,
            margin_bottom: MARGIN,
            margin_left: MARGIN,
        };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_print_setup_apply_page_rule(
    setup: *mut PrintSetup,
    rule: *const PageRule,
) {
    let (setup, rule) = match unsafe { (&mut *setup, rule.as_ref()) } {
        (setup, Some(rule)) => (setup, rule),
        _ => return,
    };
    if rule.has_size != 0 && rule.width > 1.0 && rule.height > 1.0 {
        setup.width = rule.width;
        setup.height = rule.height;
    }
    let margins = [
        &mut setup.margin_top,
        &mut setup.margin_right,
        &mut setup.margin_bottom,
        &mut setup.margin_left,
    ];
    for (i, side) in margins.into_iter().enumerate() {
        if rule.has_margin[i] != 0 {
            *side = rule.margin[i];
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_print_page_offsets(
    root: *const NsBox,
    page_content_height: f64,
) -> *mut GArray {
    let offsets = crate::page_offsets(unsafe { BoxRef::from_ptr(root) }, page_content_height);
    unsafe {
        let out = g_array_new(FALSE, FALSE, size_of::<f64>() as c_uint);
        g_array_append_vals(out, offsets.as_ptr().cast(), offsets.len() as c_uint)
    }
}

unsafe fn doubles<'a>(array: *const GArray) -> &'a [f64] {
    match unsafe { array.as_ref() } {
        Some(a) if a.len > 0 => unsafe {
            core::slice::from_raw_parts(a.data.cast::<f64>(), a.len as usize)
        },
        _ => &[],
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_print_page_bottom(
    offsets: *const GArray,
    i: c_uint,
    page_content_height: f64,
) -> f64 {
    crate::page_bottom(unsafe { doubles(offsets) }, i as usize, page_content_height)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_print_draw_page(
    cr: *mut c_void,
    root: *const NsBox,
    setup: *const PrintSetup,
    scale: f64,
    page_top: f64,
    page_bottom: f64,
) {
    let Some(setup) = (unsafe { setup.as_ref() }) else {
        return;
    };
    if cr.is_null() || root.is_null() {
        return;
    }
    let content_w = setup.width - setup.margin_left - setup.margin_right;
    let content_h = page_bottom - page_top;
    if !crate::exceeds(content_w, 0.0) || !crate::exceeds(content_h, 0.0) {
        return;
    }
    unsafe {
        cairo_save(cr);
        cairo_scale(cr, scale, scale);
        cairo_translate(cr, setup.margin_left, setup.margin_top);
        cairo_rectangle(cr, 0.0, 0.0, content_w, content_h);
        cairo_clip(cr);
        cairo_translate(cr, 0.0, -page_top);
        ns_paint(cr, root, ptr::null());
        cairo_restore(cr);
    }
}
