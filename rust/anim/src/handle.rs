//! Southstar — the handle a page owns its animation engine through and lends to the crates that sample, query and drive it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr::NonNull;

use southstar_css::StyleMap;
use southstar_css::{Declarations, OwnedValue, Sheet};
use southstar_dom::Node;
use southstar_style::Transform;

use crate::engine::{Anim, Info, ScriptTiming, TARGET_BG_COLOR, TARGET_COLOR};
use crate::ffi::css::{self, NodePtr, StylesTable};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AnimHandle(NonNull<Anim>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AnimatedColor {
    Color,
    BackgroundColor,
}

pub struct AnimEvent {
    pub node: Node<'static>,
    pub kind: &'static CStr,
    pub name: Vec<u8>,
    pub elapsed_ms: f64,
}

pub struct AnimInfo {
    pub node: Node<'static>,
    pub prop: i32,
    pub run: i32,
    pub name: Option<String>,
    pub fill: &'static str,
    pub direction: &'static str,
    pub easing: String,
    pub current_ms: f64,
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub iterations: f64,
    pub active: bool,
    pub paused: bool,
    pub pending: bool,
    pub finished: bool,
    pub generation: u32,
}

pub struct AnimKeyframe {
    pub offset: f64,
    pub easing: String,
    pub decls: Option<Declarations>,
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl AnimInfo {
    fn of(info: Info) -> Option<AnimInfo> {
        let name = match (info.name, info.prop_name) {
            (Some(name), _) => Some(lossy(&name)),
            (None, Some(prop)) => Some(prop.to_string_lossy().into_owned()),
            (None, None) => None,
        };
        Some(AnimInfo {
            node: info.node.node()?,
            prop: info.prop,
            run: info.run,
            name,
            fill: info.fill.to_str().unwrap_or("none"),
            direction: info.direction.to_str().unwrap_or("normal"),
            easing: lossy(&info.easing),
            current_ms: info.current_ms,
            duration_ms: info.duration_ms,
            delay_ms: info.delay_ms,
            iterations: info.iterations,
            active: info.active,
            paused: info.paused,
            pending: info.pending,
            finished: info.finished,
            generation: info.generation,
        })
    }
}

fn node_ptr(node: Node<'_>) -> NodePtr {
    NodePtr::new(node.as_ptr()).expect("a node has an address")
}

impl AnimHandle {
    pub fn create() -> AnimHandle {
        AnimHandle(NonNull::from(Box::leak(Box::new(Anim::new()))))
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// # Safety
    ///
    /// `raw` is null or a pointer `raw()` returned for a handle that has not been freed.
    pub unsafe fn from_raw(raw: *mut c_void) -> Option<AnimHandle> {
        NonNull::new(raw.cast()).map(AnimHandle)
    }

    /// # Safety
    ///
    /// No copy of this handle, and no pointer `raw()` returned, may be used afterwards.
    pub unsafe fn free(self) {
        // SAFETY: the engine was leaked from a Box by create(), and the caller guarantees nothing uses it again.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }

    fn with<R>(self, f: impl FnOnce(&mut Anim) -> R) -> R {
        // SAFETY: the engine stays allocated until free(), whose contract rules out later use, and no other reference to it is live while the page is on this thread calling in.
        f(unsafe { &mut *self.0.as_ptr() })
    }

    pub fn tick(self, now_us: i64) -> bool {
        self.with(|a| a.tick(now_us))
    }

    pub fn has_active(self) -> bool {
        self.with(|a| a.has_active())
    }

    pub fn needs_layout(self) -> bool {
        self.with(|a| a.needs_layout())
    }

    pub fn rebase(self, base_us: i64) {
        self.with(|a| a.rebase(base_us));
    }

    pub fn load_keyframes(self, sheet: &Sheet) {
        self.with(|a| a.load_keyframes(sheet));
    }

    pub fn observe_all(self, styles: &StyleMap, now_us: i64) {
        self.with(|a| a.observe_all(StylesTable::new(styles), now_us));
    }

    pub fn take_events(self) -> Vec<AnimEvent> {
        self.with(Anim::take_events)
            .into_iter()
            .filter_map(|e| {
                Some(AnimEvent {
                    node: e.node.node()?,
                    kind: e.kind,
                    name: e.name,
                    elapsed_ms: e.elapsed_ms,
                })
            })
            .collect()
    }

    pub fn opacity(self, node: Node<'_>) -> Option<f64> {
        self.with(|a| a.opacity(node_ptr(node)))
    }

    pub fn transform(self, node: Node<'_>) -> Option<Transform> {
        self.with(|a| a.transform(node_ptr(node)))
    }

    pub fn color(self, node: Node<'_>, which: AnimatedColor) -> Option<[u8; 4]> {
        let which = match which {
            AnimatedColor::Color => TARGET_COLOR,
            AnimatedColor::BackgroundColor => TARGET_BG_COLOR,
        };
        self.with(|a| a.color(node_ptr(node), which))
    }

    pub fn visit(self, filter: Option<Node<'_>>) -> Vec<AnimInfo> {
        self.with(|a| a.visit(filter.map(node_ptr)))
            .into_iter()
            .filter_map(AnimInfo::of)
            .collect()
    }

    pub fn info_for(self, node: Node<'_>, prop: i32) -> Option<AnimInfo> {
        self.with(|a| a.info_for(node_ptr(node), prop))
            .and_then(AnimInfo::of)
    }

    pub fn keyframes(self, node: Node<'_>, prop: i32) -> Vec<AnimKeyframe> {
        self.with(|a| a.keyframes(node_ptr(node), prop))
            .into_iter()
            .map(|k| AnimKeyframe {
                offset: k.offset,
                easing: lossy(&k.easing),
                decls: k.decls.map(|d| d.declarations().clone()),
            })
            .collect()
    }

    pub fn seek(self, node: Node<'_>, prop: i32, ms: f64) -> bool {
        self.with(|a| a.seek(node_ptr(node), prop, ms))
    }

    pub fn base_value(self, node: Node<'_>, prop: i32) -> Option<OwnedValue> {
        self.with(|a| css::Val::retain(a.base_value(node_ptr(node), prop)))
            .map(css::Val::into_owned)
    }

    pub fn control(self, node: Node<'_>, prop: i32, op: &[u8]) -> bool {
        self.with(|a| a.control(node_ptr(node), prop, op))
    }

    pub fn script_start(
        self,
        node: Node<'_>,
        stops: &[(Option<&str>, f64)],
        timing: &ScriptTiming<'_>,
    ) -> Option<(i32, u32)> {
        self.with(|a| a.script_start(node_ptr(node), stops, timing))
    }
}
