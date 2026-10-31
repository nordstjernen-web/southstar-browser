//! Southstar — the page's runtime state kept in Rust: the shell's callbacks, the scheduling depths and deadlines, the realm scope bookkeeping and the queued message tasks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_int, c_void};
use core::ptr;
use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::rc::Rc;

use southstar_css::StyleMap;
use southstar_dom::{Node, NsNode, WeakNode};
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::scopes::BaseFrame;

pub type LogFn = dyn Fn(&CStr);
pub type DataFn = dyn Fn();
pub type NavigateFn = dyn Fn(&CStr, bool);
pub type DownloadFn = dyn Fn(&CStr, Option<&CStr>);
pub type NodeFn = dyn Fn(Node<'_>);
pub type UrlFn = dyn Fn(&CStr);
pub type FormSubmitFn = dyn Fn(Node<'_>, Option<Node<'_>>);
pub type ViewportScrollFn = dyn Fn(f64, f64) -> (f64, f64);
pub type TextQueryFn = dyn Fn(&CStr) -> bool;
pub type DelayFn = dyn Fn() -> bool;
pub(crate) type TaskRun = Box<dyn FnOnce(&mut Scope<'_>) -> Result<Value, Value>>;
pub(crate) type UrlCell = Rc<RefCell<Option<CString>>>;

pub(crate) struct Hook<F: ?Sized>(RefCell<Option<Rc<F>>>);

impl<F: ?Sized> Hook<F> {
    fn new() -> Hook<F> {
        Hook(RefCell::new(None))
    }

    pub fn set(&self, f: Option<Rc<F>>) {
        drop(self.0.replace(f));
    }

    pub fn get(&self) -> Option<Rc<F>> {
        self.0.borrow().clone()
    }

    pub fn is_set(&self) -> bool {
        self.0.borrow().is_some()
    }
}

pub(crate) struct MessageTask {
    pub ctx: ContextHandle,
    pub run: TaskRun,
}

pub(crate) struct PageState {
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
    pub current_url: RefCell<Option<CString>>,
    pub document_origin: RefCell<Option<CString>>,
    pub realm_scope_base: Cell<Option<BaseFrame>>,
    pub top_url_slot: RefCell<Option<UrlCell>>,
    pub current_script: Cell<*mut NsNode>,
    pub early_inject_src: RefCell<Option<CString>>,
    pub mutated: Cell<bool>,
    pub main_context: RefCell<Option<southstar_glib::MainContext>>,
    pub worker_host: RefCell<Option<crate::api::WorkerHost>>,
    pub style_table: RefCell<Option<Rc<StyleMap>>>,
    pub autofocus_processed: Cell<bool>,
    pub active_modal: Cell<*const NsNode>,
    pub user_activation_us: Cell<i64>,
    pub user_ever_activated: Cell<bool>,
    pub image_cache: Cell<*mut c_void>,
    pub anim: RefCell<Option<southstar_anim::AnimHandle>>,
    pub ready_state: Cell<c_int>,
    pub doc_ready_states: RefCell<HashMap<WeakNode, c_int>>,
    pub message_tasks: RefCell<VecDeque<MessageTask>>,
    pub message_task_source: Cell<Option<southstar_glib::SourceId>>,
    pub eval_deadline_us: Cell<i64>,
    pub monitor_deadline_us: Cell<i64>,
    pub in_pump: Cell<bool>,
    pub eval_depth: Cell<c_int>,
    pub iframe_load_depth: Cell<c_int>,
    pub dispatch_depth: Cell<c_int>,
    pub callback_depth: Cell<c_int>,
    pub performing_microtask_checkpoint: Cell<bool>,
    pub last_orphan_sweep_us: Cell<i64>,
    pub observer_tick_source: Cell<Option<southstar_glib::SourceId>>,
    pub iframe_doc: RefCell<Option<Value>>,
    pub selection_text: RefCell<Option<CString>>,
    pub selection_has_range: Cell<bool>,
    pub selection_rect: Cell<[f64; 4]>,
    pub ignore_destructive_writes: Cell<c_int>,
    pub module_ctx: Cell<ContextHandle>,
}

impl PageState {
    pub fn new() -> PageState {
        PageState {
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
            current_url: RefCell::new(None),
            document_origin: RefCell::new(None),
            realm_scope_base: Cell::new(None),
            top_url_slot: RefCell::new(None),
            current_script: Cell::new(ptr::null_mut()),
            early_inject_src: RefCell::new(None),
            mutated: Cell::new(false),
            main_context: RefCell::new(None),
            worker_host: RefCell::new(None),
            style_table: RefCell::new(None),
            autofocus_processed: Cell::new(false),
            active_modal: Cell::new(ptr::null()),
            user_activation_us: Cell::new(0),
            user_ever_activated: Cell::new(false),
            image_cache: Cell::new(ptr::null_mut()),
            anim: RefCell::new(None),
            ready_state: Cell::new(0),
            doc_ready_states: RefCell::new(HashMap::new()),
            message_tasks: RefCell::new(VecDeque::new()),
            message_task_source: Cell::new(None),
            eval_deadline_us: Cell::new(0),
            monitor_deadline_us: Cell::new(0),
            in_pump: Cell::new(false),
            eval_depth: Cell::new(0),
            iframe_load_depth: Cell::new(0),
            dispatch_depth: Cell::new(0),
            callback_depth: Cell::new(0),
            performing_microtask_checkpoint: Cell::new(false),
            last_orphan_sweep_us: Cell::new(0),
            observer_tick_source: Cell::new(None),
            iframe_doc: RefCell::new(None),
            selection_text: RefCell::new(None),
            selection_has_range: Cell::new(false),
            selection_rect: Cell::new([0.0; 4]),
            ignore_destructive_writes: Cell::new(0),
            module_ctx: Cell::new(ContextHandle::NULL),
        }
    }
}
