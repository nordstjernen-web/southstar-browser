//! Southstar — the C ABI and Rust-ABI entry points of the shell's input events and drag sessions, and the realm, event and dispatch calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::BoxRef;

use crate::transfer::DragSession;
use crate::{Element, Pointer};

use southstar_js_runtime::api::Page;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(Option<Page>);

fn live(n: Node<'_>) -> Option<Element> {
    n.downgrade().get()
}

impl Js {
    fn of(js: Option<Page>) -> Js {
        Js(js)
    }

    fn of_page(page: Page) -> Js {
        Js(Some(page))
    }

    fn ptr(self) -> Option<Page> {
        self.0
    }

    fn page(self) -> Option<Page> {
        self.ptr().filter(|page| page.is_live())
    }

    pub fn ctx(self) -> ContextHandle {
        api::main_context(self.ptr())
    }

    pub fn has_context(self) -> bool {
        !self.ctx().is_null()
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.ctx().with(f)
    }

    pub fn realm_scope<R>(self, node: Element, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        southstar_js_frames::node_realm_context(self.ptr(), node)
            .or(api::main_realm(self.ptr()))
            .or(self.ctx())
            .with(f)
    }

    pub fn current_document(self) -> Option<Element> {
        api::current_document(self.ptr())
    }

    pub fn layout_root(self) -> Option<BoxRef<'static>> {
        api::layout_root(self.ptr())
    }

    pub fn halted(self) -> bool {
        api::halted(self.ptr())
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.ptr())
    }

    pub fn blocked(self) -> bool {
        self.halted() || self.in_pump()
    }

    pub fn wpt_hooks_enabled(self) -> bool {
        api::wpt_hooks_enabled(self.ptr())
    }

    pub fn note_user_activation(self) {
        api::note_user_activation(self.ptr());
    }

    pub fn note_pointer_input(self, pointer: bool) {
        if let Some(page) = self.page() {
            southstar_js_focus::note_pointer_input(page, pointer);
        }
    }

    pub fn path_has_active_listener(self, target: Element, kind: &str) -> bool {
        southstar_js_dispatch::path_has_active_listener(self.ptr(), target, kind)
    }

    pub fn dispatch_built(self, target: Element, kind: &str, event: Value) -> (bool, bool) {
        southstar_js_dispatch::dispatch_built_event(self.ptr(), target, kind, event)
    }

    pub fn dispatch_key(
        self,
        target: Element,
        kind: &str,
        key: &str,
        key_code: i32,
        shift: bool,
    ) -> bool {
        let press = southstar_js_dispatch::KeyPress {
            key: Some(key),
            code: Some(""),
            key_code,
            char_code: 0,
            modifiers: southstar_js_dispatch::Modifiers {
                shift,
                ..Default::default()
            },
        };
        southstar_js_dispatch::dispatch_key_event(self.ptr(), target, kind, &press).1
    }

    pub fn light_dismiss(self, target: Element, up: bool) {
        southstar_js_top_layer::popover_light_dismiss(self.ptr(), target, up);
        southstar_js_top_layer::dialog_light_dismiss(self.ptr(), target, up);
    }

    pub fn set_focus(self, el: Option<Element>) {
        if let Some(page) = self.page() {
            southstar_js_focus::set_focus(page, el);
        }
    }

    pub fn focused_node(self) -> Option<Element> {
        southstar_js_focus::focused_node(self.ptr())
    }

    pub fn sequential_focus_target(self, backward: bool) -> Option<Element> {
        southstar_js_focus::sequential_focus_target(self.page()?, backward)
    }

    pub fn keyboard_activate(self, el: Element, key: &str, keyup: bool) {
        let key = key.replace('\0', " ");
        if let Some(page) = self.page() {
            southstar_js_focus::keyboard_activate(page, el, key.as_bytes(), keyup);
        }
    }

    pub fn process_close_request(self) {
        southstar_js_top_layer::process_close_request(self.ptr());
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(Page::current(scope))
}

pub(crate) fn unwrap(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap(scope: &mut Scope<'_>, el: Element) -> Value {
    api::make_element(scope, el.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str, target: Element) -> Value {
    let kind = kind.replace('\0', " ");
    southstar_js_events::make_event(scope, &kind, Some(target))
}

pub(crate) fn realm_now_ms(scope: &Scope<'_>) -> f64 {
    southstar_js_perf::context_now_ms(scope.handle())
}

pub(crate) fn is_focusable(el: Element) -> bool {
    southstar_js_focus::is_focusable(el)
}

pub(crate) fn find_box<'a>(root: BoxRef<'a>, node: Element) -> Option<BoxRef<'a>> {
    southstar_js_geometry::find_box_by_dom(root, node.as_ptr().cast())
}

pub(crate) fn visual_border_box(b: BoxRef<'_>) -> (f64, f64) {
    let rect = southstar_js_geometry::box_visual_border_box(b);
    (rect.x, rect.y)
}

pub(crate) struct FileBytes {
    pub contents: Vec<u8>,
    pub mime: Option<String>,
}

pub(crate) fn read_file(path: &CStr) -> Option<FileBytes> {
    let contents = southstar_glib::file_get_contents(path.to_bytes())?;
    let mime = southstar_mime::guess(path.to_bytes(), Some(&contents)).map(str::to_owned);
    Some(FileBytes { contents, mime })
}

pub fn dispatch_mouse_event(
    js: Page,
    target: Node<'_>,
    kind: &CStr,
    input: &Pointer,
    related: Option<Node<'_>>,
) -> (bool, bool) {
    let js = Js::of_page(js);
    let Some(target) = live(target) else {
        return (false, false);
    };
    let kind = kind.to_string_lossy();
    crate::mouse::mouse_event(js, target, &kind, input, related.and_then(live))
}

pub fn dispatch_clipboard_event(
    js: Page,
    target: Node<'_>,
    kind: &CStr,
    text: Option<&CStr>,
) -> (bool, bool) {
    let js = Js::of_page(js);
    let Some(target) = live(target) else {
        return (false, false);
    };
    let kind = kind.to_string_lossy();
    let text = text.map(CStr::to_string_lossy);
    crate::transfer::clipboard_event(js, target, &kind, text.as_deref())
}

pub struct DragData(Option<DragSession>);

impl DragData {
    pub fn new(js: Page) -> Option<DragData> {
        DragSession::new(Js::of_page(js)).map(|session| DragData(Some(session)))
    }

    pub fn set_data(&self, kind: &CStr, data: Option<&CStr>) {
        if let Some(session) = &self.0 {
            let data = data.map(CStr::to_string_lossy);
            session.set_data(&kind.to_string_lossy(), data.as_deref());
        }
    }

    pub fn add_file(&self, path: &CStr) {
        if let Some(session) = &self.0 {
            session.add_file(path);
        }
    }
}

impl Drop for DragData {
    fn drop(&mut self) {
        if let Some(session) = self.0.take() {
            session.release();
        }
    }
}

pub fn dispatch_drag_event(
    js: Page,
    session: Option<&DragData>,
    target: Node<'_>,
    kind: &CStr,
    input: &Pointer,
    related: Option<Node<'_>>,
) -> (bool, bool) {
    let js = Js::of_page(js);
    let Some(target) = live(target) else {
        return (false, false);
    };
    let kind = kind.to_string_lossy();
    let session = session.and_then(|data| data.0.as_ref());
    crate::transfer::drag_event(js, session, target, &kind, input, related.and_then(live))
}

pub fn nsr_js_input_install_wpt(scope: &mut Scope<'_>, global: &Value) {
    crate::wpt::install(scope, global);
}

macro_rules! capture_methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(
                scope: &mut Scope<'_>,
                this: &Value,
                args: &[Value],
            ) -> Result<Value, Value> {
                $f(scope, this, args)
            }
        )*
    };
}

capture_methods! {}

fn teardown(js: Option<Page>) {
    crate::teardown(Js::of(js));
}

pub fn install_runtime_hooks() {
    api::set_input_hooks(api::PageStateHooks {
        init: |_| {},
        teardown: |js| teardown(Some(js)),
    });
    southstar_js_runtime::links::set_js_input_links(southstar_js_runtime::links::JsInputLinks {
        js_input_install_wpt: crate::ffi::nsr_js_input_install_wpt,
    });
}
