//! Southstar — the C ABI of src/anim.h over the animation engine, with every callback made after the engine is released so script can call back in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod css;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable};
use southstar_layout::Style;
use southstar_style::{NsCssValue, StyleRef};

use crate::engine::{Anim, Info, ScriptTiming};
use css::{NodePtr, StylesTable};

type EventCb = Option<
    unsafe extern "C" fn(
        node: *const NsNode,
        kind: *const c_char,
        name: *const c_char,
        elapsed_ms: f64,
        user: *mut c_void,
    ),
>;
type VisitCb = Option<unsafe extern "C" fn(info: *const NsAnimInfo, user: *mut c_void)>;
type KeyframeCb = Option<
    unsafe extern "C" fn(
        offset: f64,
        easing: *const c_char,
        decls: *const GArray,
        user: *mut c_void,
    ),
>;

const EASING_LEN: usize = 96;

#[repr(C)]
pub struct NsAnimInfo {
    node: *const NsNode,
    prop: c_int,
    run: c_int,
    name: *const c_char,
    fill: *const c_char,
    direction: *const c_char,
    easing: [c_char; EASING_LEN],
    current_ms: f64,
    duration_ms: f64,
    delay_ms: f64,
    iterations: f64,
    active: GBoolean,
    paused: GBoolean,
    pending: GBoolean,
    finished: GBoolean,
    generation: c_uint,
}

#[repr(C)]
pub struct NsAnimScriptTiming {
    duration_ms: f64,
    delay_ms: f64,
    iterations: f64,
    direction: *const c_char,
    fill: *const c_char,
    easing: *const c_char,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsAnimInfo>() == 192
        && core::mem::offset_of!(NsAnimInfo, easing) == 40
        && core::mem::offset_of!(NsAnimInfo, current_ms) == 136
        && core::mem::offset_of!(NsAnimInfo, active) == 168
        && core::mem::offset_of!(NsAnimInfo, generation) == 184
        && core::mem::size_of::<NsAnimScriptTiming>() == 48
);

fn engine<'a>(a: *mut Anim) -> Option<&'a mut Anim> {
    unsafe { a.as_mut() }
}

fn text<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn style<'a>(p: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(p) }
}

impl NsAnimInfo {
    fn from(info: &Info) -> NsAnimInfo {
        let mut easing = [0 as c_char; EASING_LEN];
        for (dst, &src) in easing
            .iter_mut()
            .zip(info.easing.iter().take(EASING_LEN - 1))
        {
            *dst = src as c_char;
        }
        let name = match (&info.name, info.prop_name) {
            (Some(name), _) => css::intern(&cstring(name)),
            (None, Some(prop)) => prop.as_ptr(),
            (None, None) => ptr::null(),
        };
        NsAnimInfo {
            node: info.node.as_ptr(),
            prop: info.prop,
            run: info.run,
            name,
            fill: info.fill.as_ptr(),
            direction: info.direction.as_ptr(),
            easing,
            current_ms: info.current_ms,
            duration_ms: info.duration_ms,
            delay_ms: info.delay_ms,
            iterations: info.iterations,
            active: glib::boolean(info.active),
            paused: glib::boolean(info.paused),
            pending: glib::boolean(info.pending),
            finished: glib::boolean(info.finished),
            generation: info.generation,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_anim_new() -> *mut Anim {
    Box::into_raw(Box::new(Anim::new()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_free(a: *mut Anim) {
    if !a.is_null() {
        drop(unsafe { Box::from_raw(a) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_load_from_stylesheet(a: *mut Anim, sheet: *const c_void) {
    if let (Some(a), false) = (engine(a), sheet.is_null()) {
        a.load_keyframes(sheet);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_observe(
    a: *mut Anim,
    dom: *const NsNode,
    style_ptr: *const Style,
    now_us: i64,
) {
    if let (Some(a), Some(node), Some(style)) = (engine(a), NodePtr::new(dom), style(style_ptr)) {
        a.observe(node, style, now_us, None);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_observe_all(a: *mut Anim, styles: *mut GHashTable, now_us: i64) {
    if let (Some(a), Some(styles)) = (engine(a), StylesTable::new(styles)) {
        a.observe_all(styles, now_us);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_apply(a: *mut Anim, styles: *mut GHashTable) {
    if let (Some(a), Some(styles)) = (engine(a), StylesTable::new(styles)) {
        a.apply(styles);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_tick(a: *mut Anim, now_us: i64) -> GBoolean {
    glib::boolean(engine(a).is_some_and(|a| a.tick(now_us)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_has_active(a: *const Anim) -> GBoolean {
    glib::boolean(unsafe { a.as_ref() }.is_some_and(Anim::has_active))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_needs_layout(a: *const Anim) -> GBoolean {
    glib::boolean(unsafe { a.as_ref() }.is_some_and(Anim::needs_layout))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_drain_events(a: *mut Anim, cb: EventCb, user: *mut c_void) {
    let Some(a) = engine(a) else {
        return;
    };
    let events = a.take_events();
    let Some(cb) = cb else {
        return;
    };
    for e in events {
        let name = cstring(&e.name);
        unsafe {
            cb(
                e.node.as_ptr(),
                e.kind.as_ptr(),
                name.as_ptr(),
                e.elapsed_ms,
                user,
            )
        };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_get_opacity(
    a: *mut Anim,
    dom: *const NsNode,
    out_opacity: *mut f64,
) -> GBoolean {
    let (Some(a), Some(node), false) = (engine(a), NodePtr::new(dom), out_opacity.is_null()) else {
        return 0;
    };
    match a.opacity(node) {
        Some(o) => {
            unsafe { *out_opacity = o };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_get_transform(a: *mut Anim, dom: *const NsNode) -> *const c_void {
    let (Some(a), Some(node)) = (engine(a), NodePtr::new(dom)) else {
        return ptr::null();
    };
    a.transform(node).unwrap_or(ptr::null())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_get_color(
    a: *mut Anim,
    dom: *const NsNode,
    which: c_int,
    out_rgba: *mut u8,
) -> GBoolean {
    let (Some(a), Some(node), false) = (engine(a), NodePtr::new(dom), out_rgba.is_null()) else {
        return 0;
    };
    match a.color(node, which) {
        Some(rgba) => {
            unsafe { ptr::copy_nonoverlapping(rgba.as_ptr(), out_rgba, 4) };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_visit(
    a: *mut Anim,
    node: *const NsNode,
    cb: VisitCb,
    user: *mut c_void,
) {
    let (Some(a), Some(cb)) = (engine(a), cb) else {
        return;
    };
    let infos: Vec<NsAnimInfo> = a
        .visit(NodePtr::new(node))
        .iter()
        .map(NsAnimInfo::from)
        .collect();
    for info in &infos {
        unsafe { cb(info, user) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_info_for(
    a: *mut Anim,
    node: *const NsNode,
    prop: c_int,
    out: *mut NsAnimInfo,
) -> GBoolean {
    let (Some(a), Some(node), false) = (engine(a), NodePtr::new(node), out.is_null()) else {
        return 0;
    };
    match a.info_for(node, prop) {
        Some(info) => {
            unsafe { out.write(NsAnimInfo::from(&info)) };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_keyframes_visit(
    a: *mut Anim,
    node: *const NsNode,
    prop: c_int,
    cb: KeyframeCb,
    user: *mut c_void,
) {
    let (Some(a), Some(node), Some(cb)) = (engine(a), NodePtr::new(node), cb) else {
        return;
    };
    let copies = a.keyframes(node, prop);
    let easings: Vec<CString> = copies.iter().map(|k| cstring(&k.easing)).collect();
    for (copy, easing) in copies.iter().zip(&easings) {
        let decls = copy.decls.as_ref().map_or(ptr::null(), css::Decls::as_ptr);
        unsafe { cb(copy.offset, easing.as_ptr(), decls, user) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_seek(
    a: *mut Anim,
    node: *const NsNode,
    prop: c_int,
    ms: f64,
) -> GBoolean {
    let (Some(a), Some(node)) = (engine(a), NodePtr::new(node)) else {
        return 0;
    };
    glib::boolean(a.seek(node, prop, ms))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_base_value(
    a: *mut Anim,
    node: *const NsNode,
    prop: c_int,
) -> *const NsCssValue {
    let (Some(a), Some(node)) = (engine(a), NodePtr::new(node)) else {
        return ptr::null();
    };
    a.base_value(node, prop).as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_control(
    a: *mut Anim,
    node: *const NsNode,
    prop: c_int,
    op: *const c_char,
) -> GBoolean {
    let (Some(a), Some(node), Some(op)) = (engine(a), NodePtr::new(node), text(op)) else {
        return 0;
    };
    glib::boolean(a.control(node, prop, op.to_bytes()))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_anim_script_start(
    a: *mut Anim,
    node: *const NsNode,
    stop_css: *const *const c_char,
    stop_pct: *const f64,
    n_stops: c_int,
    t: *const NsAnimScriptTiming,
    out_prop: *mut c_int,
    out_generation: *mut c_uint,
) -> GBoolean {
    let (Some(a), Some(node), Some(t)) = (engine(a), NodePtr::new(node), unsafe { t.as_ref() })
    else {
        return 0;
    };
    let n = usize::try_from(n_stops).unwrap_or(0);
    let stops: Vec<(Option<&CStr>, f64)> = (0..n)
        .map(|i| unsafe { (text(*stop_css.add(i)), *stop_pct.add(i)) })
        .collect();
    let timing = ScriptTiming {
        duration_ms: t.duration_ms,
        delay_ms: t.delay_ms,
        iterations: t.iterations,
        direction: text(t.direction),
        fill: text(t.fill),
        easing: text(t.easing),
    };
    let Some((prop, generation)) = a.script_start(node, &stops, &timing) else {
        return 0;
    };
    unsafe {
        if let Some(p) = out_prop.as_mut() {
            *p = prop;
        }
        if let Some(g) = out_generation.as_mut() {
            *g = generation;
        }
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_prune(a: *mut Anim, live: *mut GHashTable) {
    if let (Some(a), Some(live)) = (engine(a), StylesTable::new(live)) {
        a.prune(live);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_anim_rebase(a: *mut Anim, base_us: i64) {
    if let Some(a) = engine(a) {
        a.rebase(base_us);
    }
}
