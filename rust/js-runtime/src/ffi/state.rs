//! Southstar — the page's runtime state kept in Rust: the shell's callbacks, the scheduling depths and deadlines, the realm scope bookkeeping and the queued message tasks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;
use std::collections::{HashMap, VecDeque};

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_js_engine::{ContextHandle, RuntimeHandle, Scope, Value};

use super::scopes::RealmScope;
use super::{GMainContext, WorkerHost};

pub type LogFn = unsafe extern "C" fn(line: *const c_char, data: *mut c_void);
pub type DataFn = unsafe extern "C" fn(data: *mut c_void);
pub type NavigateFn = unsafe extern "C" fn(url: *const c_char, reload: GBoolean, data: *mut c_void);
pub type DownloadFn =
    unsafe extern "C" fn(url: *const c_char, filename: *const c_char, data: *mut c_void);
pub type NodeFn = unsafe extern "C" fn(target: *const NsNode, data: *mut c_void);
pub type UrlFn = unsafe extern "C" fn(url: *const c_char, data: *mut c_void);
pub type FormSubmitFn =
    unsafe extern "C" fn(form: *const NsNode, submitter: *const NsNode, data: *mut c_void);
pub type ViewportScrollFn = unsafe extern "C" fn(x: *mut f64, y: *mut f64, data: *mut c_void);
pub type TextQueryFn = unsafe extern "C" fn(text: *const c_char, data: *mut c_void) -> GBoolean;
pub type DelayFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;
pub(crate) type TaskRun = Box<dyn FnOnce(&mut Scope<'_>) -> Result<Value, Value>>;

#[repr(C)]
pub(crate) struct Slots {
    pub rt: *mut RuntimeHandle,
    pub ctx: *mut ContextHandle,
    pub main_realm_ctx: *mut ContextHandle,
    pub current_url: *mut *mut c_char,
    pub current_doc: *mut *mut NsNode,
    pub raf_frame_ctx: *mut *mut NsNode,
    pub ce_main_doc: *mut *const NsNode,
    pub halted: *mut GBoolean,
}

pub(crate) struct Hook<F: Copy> {
    pub f: Cell<Option<F>>,
    pub data: Cell<*mut c_void>,
}

impl<F: Copy> Hook<F> {
    fn new() -> Hook<F> {
        Hook {
            f: Cell::new(None),
            data: Cell::new(ptr::null_mut()),
        }
    }

    pub fn set(&self, f: Option<F>, data: *mut c_void) {
        self.f.set(f);
        self.data.set(data);
    }

    pub fn get(&self) -> Option<(F, *mut c_void)> {
        self.f.get().map(|f| (f, self.data.get()))
    }

    pub fn is_set(&self) -> bool {
        self.f.get().is_some()
    }
}

pub(crate) struct MessageTask {
    pub ctx: ContextHandle,
    pub run: TaskRun,
}

pub(crate) struct PageState {
    pub slots: Slots,
    pub log: Hook<LogFn>,
    pub mutated_hook: Hook<DataFn>,
    pub navigate: Hook<NavigateFn>,
    pub download: Hook<DownloadFn>,
    pub scroll_to: Hook<NodeFn>,
    pub fragment_nav: Hook<UrlFn>,
    pub form_submit: Hook<FormSubmitFn>,
    pub soft_nav: Hook<NavigateFn>,
    pub repaint: Hook<DataFn>,
    pub layout_flush: Hook<DataFn>,
    pub viewport_scroll: Hook<ViewportScrollFn>,
    pub load_delay: Hook<DelayFn>,
    pub clipboard_write: Hook<TextQueryFn>,
    pub selection_cmd: Hook<TextQueryFn>,
    pub window_action: Hook<UrlFn>,
    pub in_layout_flush: Cell<bool>,
    pub document_origin: Cell<*mut c_char>,
    pub realm_scope_base: Cell<*mut RealmScope>,
    pub top_url_slot: Cell<*mut *mut c_char>,
    pub current_script: Cell<*mut NsNode>,
    pub early_inject_src: Cell<*mut c_char>,
    pub mutated: Cell<bool>,
    pub main_context: Cell<*mut GMainContext>,
    pub worker_host: Cell<*mut WorkerHost>,
    pub style_table: Cell<*mut GHashTable>,
    pub autofocus_processed: Cell<bool>,
    pub active_modal: Cell<*const NsNode>,
    pub user_activation_us: Cell<i64>,
    pub user_ever_activated: Cell<bool>,
    pub image_cache: Cell<*mut c_void>,
    pub anim: Cell<*mut c_void>,
    pub ready_state: Cell<c_int>,
    pub doc_ready_states: RefCell<HashMap<usize, c_int>>,
    pub message_tasks: RefCell<VecDeque<MessageTask>>,
    pub message_task_source: Cell<c_uint>,
    pub eval_deadline_us: Cell<i64>,
    pub monitor_deadline_us: Cell<i64>,
    pub in_pump: Cell<bool>,
    pub eval_depth: Cell<c_int>,
    pub iframe_load_depth: Cell<c_int>,
    pub dispatch_depth: Cell<c_int>,
    pub callback_depth: Cell<c_int>,
    pub performing_microtask_checkpoint: Cell<bool>,
    pub last_orphan_sweep_us: Cell<i64>,
    pub observer_tick_source: Cell<c_uint>,
    pub iframe_doc: RefCell<Option<Value>>,
    pub selection_text: Cell<*mut c_char>,
    pub selection_has_range: Cell<bool>,
    pub selection_rect: Cell<[f64; 4]>,
    pub ignore_destructive_writes: Cell<c_int>,
    pub module_ctx: Cell<ContextHandle>,
}

impl PageState {
    pub fn new(slots: Slots) -> PageState {
        PageState {
            slots,
            log: Hook::new(),
            mutated_hook: Hook::new(),
            navigate: Hook::new(),
            download: Hook::new(),
            scroll_to: Hook::new(),
            fragment_nav: Hook::new(),
            form_submit: Hook::new(),
            soft_nav: Hook::new(),
            repaint: Hook::new(),
            layout_flush: Hook::new(),
            viewport_scroll: Hook::new(),
            load_delay: Hook::new(),
            clipboard_write: Hook::new(),
            selection_cmd: Hook::new(),
            window_action: Hook::new(),
            in_layout_flush: Cell::new(false),
            document_origin: Cell::new(ptr::null_mut()),
            realm_scope_base: Cell::new(ptr::null_mut()),
            top_url_slot: Cell::new(ptr::null_mut()),
            current_script: Cell::new(ptr::null_mut()),
            early_inject_src: Cell::new(ptr::null_mut()),
            mutated: Cell::new(false),
            main_context: Cell::new(ptr::null_mut()),
            worker_host: Cell::new(ptr::null_mut()),
            style_table: Cell::new(ptr::null_mut()),
            autofocus_processed: Cell::new(false),
            active_modal: Cell::new(ptr::null()),
            user_activation_us: Cell::new(0),
            user_ever_activated: Cell::new(false),
            image_cache: Cell::new(ptr::null_mut()),
            anim: Cell::new(ptr::null_mut()),
            ready_state: Cell::new(0),
            doc_ready_states: RefCell::new(HashMap::new()),
            message_tasks: RefCell::new(VecDeque::new()),
            message_task_source: Cell::new(0),
            eval_deadline_us: Cell::new(0),
            monitor_deadline_us: Cell::new(0),
            in_pump: Cell::new(false),
            eval_depth: Cell::new(0),
            iframe_load_depth: Cell::new(0),
            dispatch_depth: Cell::new(0),
            callback_depth: Cell::new(0),
            performing_microtask_checkpoint: Cell::new(false),
            last_orphan_sweep_us: Cell::new(0),
            observer_tick_source: Cell::new(0),
            iframe_doc: RefCell::new(None),
            selection_text: Cell::new(ptr::null_mut()),
            selection_has_range: Cell::new(false),
            selection_rect: Cell::new([0.0; 4]),
            ignore_destructive_writes: Cell::new(0),
            module_ctx: Cell::new(ContextHandle::NULL),
        }
    }

    pub fn replace_owned(slot: &Cell<*mut c_char>, value: *mut c_char) {
        let previous = slot.replace(value);
        unsafe { glib::g_free(previous.cast()) };
    }
}

impl Drop for PageState {
    fn drop(&mut self) {
        for slot in [
            &self.document_origin,
            &self.early_inject_src,
            &self.selection_text,
        ] {
            PageState::replace_owned(slot, ptr::null_mut());
        }
    }
}
