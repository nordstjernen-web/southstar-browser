//! Southstar — the animation and transition events drained from the animation engine, and the load events of images and videos layout has finished.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_anim::{AnimEvent, AnimHandle};
use southstar_dom::{Node, NsNode};
use southstar_js_engine::{Scope, Value};
use southstar_layout::{BoxRef, NsBox};

use crate::ffi::{self, Js};

const IMG_LOAD_FIRED: u32 = 1 << 3;

fn legacy_name(kind: &[u8]) -> Option<&'static CStr> {
    match kind {
        b"transitionend" => Some(c"webkitTransitionEnd"),
        b"animationstart" => Some(c"webkitAnimationStart"),
        b"animationend" => Some(c"webkitAnimationEnd"),
        b"animationiteration" => Some(c"webkitAnimationIteration"),
        _ => None,
    }
}

fn atoi(text: &[u8]) -> i32 {
    let mut rest = text.trim_ascii_start();
    let negative = match rest.first() {
        Some(b'-') => {
            rest = &rest[1..];
            true
        }
        Some(b'+') => {
            rest = &rest[1..];
            false
        }
        _ => false,
    };
    let magnitude = rest
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0i32, |n, &c| {
            n.wrapping_mul(10).wrapping_add(i32::from(c - b'0'))
        });
    if negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    }
}

fn script_event_hook(scope: &mut Scope<'_>, node: *const NsNode, kind: &[u8], name: &[u8]) {
    let global = scope.global();
    let Ok(hook) = scope.get(&global, "__ns_anim_script_event") else {
        return;
    };
    if !scope.is_function(&hook) {
        return;
    }
    let element = ffi::make_element(scope, node);
    let index = Value::int(atoi(name));
    let phase = scope.string(&String::from_utf8_lossy(&kind[4..]));
    let _ = scope.call(&hook, &Value::undefined(), &[element, index, phase]);
}

fn animation_event(
    scope: &mut Scope<'_>,
    node: *const NsNode,
    event_kind: &CStr,
    base_kind: &[u8],
    name: &[u8],
    elapsed_ms: f64,
) -> Value {
    let event = ffi::make_event(scope, event_kind, node);
    let _ = scope.set(&event, "bubbles", Value::boolean(true));
    let _ = scope.set(&event, "cancelable", Value::boolean(false));
    let _ = scope.set(&event, "elapsedTime", Value::number(elapsed_ms / 1000.0));
    let empty = scope.string("");
    let _ = scope.set(&event, "pseudoElement", empty);
    let name = scope.string(&String::from_utf8_lossy(name));
    let key = if base_kind.first() == Some(&b'a') {
        "animationName"
    } else {
        "propertyName"
    };
    let _ = scope.set(&event, key, name);
    event
}

fn anim_event(js: Js, event: &AnimEvent) {
    let ctx = js.ctx();
    if js.is_null() || ctx.is_null() || js.halted() {
        return;
    }
    let node = event.node.as_ptr();
    let kind = event.kind;
    let name = event.name.as_slice();
    let elapsed_ms = event.elapsed_ms;
    let kind_bytes = kind.to_bytes();
    if kind_bytes.starts_with(b"__ns") {
        ctx.with(|scope| script_event_hook(scope, node, kind_bytes, name));
        return;
    }
    let Some(event) =
        ctx.with(|scope| animation_event(scope, node, kind, kind_bytes, name, elapsed_ms))
    else {
        return;
    };
    js.dispatch_built(node, kind, event);
    let Some(legacy) = legacy_name(kind_bytes) else {
        return;
    };
    let Some(event) = ctx.with(|scope| {
        let event = animation_event(scope, node, legacy, kind_bytes, name, elapsed_ms);
        let base = scope.string(&kind.to_string_lossy());
        let _ = scope.set(&event, "__ns_alias_base", base);
        event
    }) else {
        return;
    };
    js.dispatch_built(node, legacy, event);
}

pub(crate) fn dispatch_anim_events(js: Js, anim: AnimHandle) {
    if js.is_null() || js.halted() || js.in_pump() {
        return;
    }
    for event in anim.take_events() {
        anim_event(js, &event);
    }
}

#[derive(Default)]
struct MediaEvents {
    images: Vec<(Node<'static>, bool)>,
    videos: Vec<*mut NsNode>,
}

fn collect_media_events(b: BoxRef<'_>, out: &mut MediaEvents) {
    let dom = b.dom_ptr().cast::<NsNode>();
    if let Some(node) = ffi::node(dom)
        && let Some(name) = node.name()
        && let Some(media) = b.media()
    {
        let dom = dom.cast_mut();
        let image = media.image();
        if name == c"img" && !image.is_null() {
            if let Some(failed) = ffi::image_settled(image)
                && node.flags() & IMG_LOAD_FIRED == 0
                && !out.images.iter().any(|&(img, _)| img == node)
            {
                out.images.push((node, failed));
            }
        } else if name == c"video"
            && southstar_video::BoxVideo::of(b)
                .is_some_and(southstar_video::BoxVideo::take_load_events)
        {
            out.videos.push(dom);
        }
    }
    let mut child = b.first_child();
    while let Some(c) = child {
        collect_media_events(c, out);
        child = c.next_sibling();
    }
    for atomic in b.inline_atomic_boxes() {
        collect_media_events(atomic, out);
    }
}

pub(crate) fn fire_media_load_events(js: Js, layout: *const NsBox) {
    if js.is_null() {
        return;
    }
    let Some(root) = ffi::layout_box(layout) else {
        return;
    };
    if js.halted() || js.in_pump() {
        return;
    }
    let mut events = MediaEvents::default();
    collect_media_events(root, &mut events);
    for (img, failed) in events.images {
        js.fire_img_load_once(img, failed);
    }
    for video in events.videos {
        js.dispatch(video, c"loadedmetadata");
        js.dispatch(video, c"loadeddata");
        js.dispatch(video, c"canplay");
    }
}
