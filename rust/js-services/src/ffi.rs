//! Southstar — the window services' nsr_ entry points and C helpers, and the js.c, network and system calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint, c_void};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api::Page;
use southstar_js_runtime::api::{self, TimerGate};

use crate::screen;
use crate::{
    caches, camera, clipboard_item, commands, console, media, navigator, rtc, timers, window,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(ContextHandle);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Frame(Option<Node<'static>>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Source(c_uint);

impl Source {
    pub const NONE: Source = Source(0);

    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

pub(crate) enum Gate {
    Run,
    Wait,
    Drop,
}

struct TimerTag {
    js: Option<Page>,
    id: i32,
}

const G_PRIORITY_DEFAULT: c_int = 0;

type SourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_timeout_source_new(interval: c_uint) -> *mut c_void;
    fn g_source_set_callback(
        source: *mut c_void,
        func: SourceFunc,
        data: *mut c_void,
        notify: glib::GDestroyNotify,
    );
    fn g_source_attach(source: *mut c_void, context: *mut c_void) -> c_uint;
    fn g_source_unref(source: *mut c_void);
    fn g_timeout_add_full(
        priority: c_int,
        interval: c_uint,
        func: SourceFunc,
        data: *mut c_void,
        notify: glib::GDestroyNotify,
    ) -> c_uint;
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn realm_of(scope: &Scope<'_>) -> Realm {
    Realm(scope.handle())
}

pub(crate) fn context_frame(scope: &Scope<'_>, js: Js) -> Frame {
    Frame(api::context_frame(js.0, scope.handle()))
}

pub(crate) fn this_is_detached_window(scope: &mut Scope<'_>, js: Js, this: &Value) -> bool {
    api::this_is_detached_window(js.0, scope, this)
}

pub(crate) fn idle_frame_end(js: Js, now: i64, end: i64) -> i64 {
    (api::lifecycle_hooks().idle_frame_end)(js.0, now, end)
}

pub(crate) fn due_timers_allowed(js: Js) -> bool {
    api::due_timers_allowed(js.0)
}

pub(crate) fn timer_gate(js: Js, frame: Frame, idle_expired: bool) -> Gate {
    match api::timer_gate(js.0, frame.0, idle_expired) {
        TimerGate::Wait => Gate::Wait,
        TimerGate::Drop => Gate::Drop,
        TimerGate::Run => Gate::Run,
    }
}

pub(crate) fn in_timer_scope(
    js: Js,
    realm: Realm,
    frame: Frame,
    f: impl FnOnce(&mut Scope<'_>) -> Result<Value, Value>,
) {
    let Some(timer_scope) = api::enter_timer_scope(js.0, realm.0, frame.0) else {
        return;
    };
    let result = timer_scope
        .context()
        .with(f)
        .unwrap_or_else(|| Ok(Value::undefined()));
    timer_scope.leave(result.err());
}

unsafe extern "C" fn timer_source_fired(data: *mut c_void) -> GBoolean {
    let tag = unsafe { &*data.cast::<TimerTag>() };
    let (js, id) = (Js(tag.js), tag.id);
    glib::boolean(timers::fire(js, id))
}

unsafe extern "C" fn timer_tag_free(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<TimerTag>()) });
}

fn timer_tag(js: Js, id: i32) -> *mut c_void {
    Box::into_raw(Box::new(TimerTag { js: js.0, id })).cast()
}

pub(crate) fn attach_timeout(js: Js, ms: u32, id: i32) -> Source {
    unsafe {
        let source = g_timeout_source_new(ms);
        g_source_set_callback(
            source,
            timer_source_fired,
            timer_tag(js, id),
            Some(timer_tag_free),
        );
        let attached = g_source_attach(source, api::glib_context(js.0));
        g_source_unref(source);
        Source(attached)
    }
}

pub(crate) fn attach_default_timeout(ms: u32, js: Js, id: i32) -> Source {
    Source(unsafe {
        g_timeout_add_full(
            G_PRIORITY_DEFAULT,
            ms,
            timer_source_fired,
            timer_tag(js, id),
            Some(timer_tag_free),
        )
    })
}

pub(crate) fn source_remove(js: Js, source: Source) {
    if !source.is_none() {
        api::source_remove(js.0, source.0);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Js(Option<Page>);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_none()
    }

    pub fn key(self) -> usize {
        self.0.map_or(0, Page::addr)
    }

    pub fn camera_permission(self) -> Option<bool> {
        southstar_camera::page_permission(self.0)
    }
}

pub(crate) struct DisplayMetrics {
    pub width: i32,
    pub height: i32,
    pub work_area: Option<(i32, i32, i32, i32)>,
}

pub(crate) enum CMethod {
    DispatchEvent,
    SendBeacon,
    EmeRequestAccess,
    MediaCapabilitiesInfo,
}

use crate::links::nsr_eme_request_access;
use crate::links::nsr_event_adopt_interface;
use crate::links::nsr_media_capabilities_info;
use crate::links::nsr_target_dispatch_with_event;
use crate::links::nsr_target_dispatchEvent;
use crate::links::nsr_target_fire_event;
use crate::links::nsr_target_make_event;
use southstar_js_net::nsr_navigator_sendBeacon;

fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

pub(crate) fn log_enabled(js: Js) -> bool {
    api::log_enabled(js.0)
}

pub(crate) fn log_line(js: Js, line: &str) {
    api::log_line(js.0, &c_string(line));
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    api::main_context(js.0).non_null()?.with(f)
}

pub(crate) fn fire_event(scope: &mut Scope<'_>, target: &Value, kind: &str) {
    nsr_target_fire_event(scope, target, kind);
}

pub(crate) fn make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    nsr_target_make_event(scope, target, kind)
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    nsr_event_adopt_interface(scope, event, iface);
}

pub(crate) fn dispatch_with_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    event: &Value,
) {
    nsr_target_dispatch_with_event(scope, target, kind, event);
}

fn until_nul(text: &str) -> &[u8] {
    let text = text.as_bytes();
    &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())]
}

pub(crate) fn media_query_matches(query: Option<&str>) -> bool {
    query.is_none_or(|q| southstar_css_media::query_matches(until_nul(q)))
}

pub(crate) fn media_list_serialize(query: Option<&str>) -> String {
    let query = query.map(until_nul).unwrap_or_default();
    String::from_utf8_lossy(&southstar_css_media::list_serialize(query)).into_owned()
}

pub(crate) fn set_device_size(width: i32, height: i32) {
    southstar_css_media::set_device_size(f64::from(width), f64::from(height));
}

#[cfg(windows)]
pub(crate) fn display_metrics() -> Option<DisplayMetrics> {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    const SM_CXSCREEN: c_int = 0;
    const SM_CYSCREEN: c_int = 1;
    const SPI_GETWORKAREA: c_uint = 0x0030;
    unsafe extern "system" {
        fn GetSystemMetrics(index: c_int) -> c_int;
        fn SystemParametersInfoW(
            action: c_uint,
            param: c_uint,
            pv_param: *mut core::ffi::c_void,
            win_ini: c_uint,
        ) -> c_int;
    }
    let mut area = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let has_area =
        unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&raw mut area).cast(), 0) } != 0;
    Some(DisplayMetrics {
        width: unsafe { GetSystemMetrics(SM_CXSCREEN) },
        height: unsafe { GetSystemMetrics(SM_CYSCREEN) },
        work_area: has_area.then_some((area.left, area.top, area.right, area.bottom)),
    })
}

#[cfg(not(windows))]
pub(crate) fn display_metrics() -> Option<DisplayMetrics> {
    None
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(Page::current(scope))
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    api::bind_event_target_listeners(scope, object);
}

pub(crate) fn c_method(scope: &mut Scope<'_>, name: &str, arity: u32, method: CMethod) -> Value {
    let f: southstar_js_engine::NativeFn = match method {
        CMethod::DispatchEvent => nsr_target_dispatchEvent,
        CMethod::SendBeacon => nsr_navigator_sendBeacon,
        CMethod::EmeRequestAccess => nsr_eme_request_access,
        CMethod::MediaCapabilitiesInfo => nsr_media_capabilities_info,
    };
    scope.function(name, arity, f)
}

pub(crate) fn user_activation(js: Js) -> (bool, bool) {
    api::user_activation_state(js.0)
}

pub(crate) fn clipboard_write(js: Js, text: &str) -> Option<bool> {
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    api::clipboard_write(js.0, &text)
}

pub(crate) fn selection_copyable(js: Js) -> bool {
    api::selection_copyable(js.0)
}

pub(crate) fn copy_selection(js: Js) -> bool {
    api::copy_selection(js.0)
}

pub(crate) fn has_selection_commands(js: Js) -> bool {
    api::has_selection_commands(js.0)
}

pub(crate) fn selection_command(js: Js, cmd: &core::ffi::CStr) -> bool {
    api::selection_command(js.0, cmd)
}

pub(crate) fn languages() -> Vec<String> {
    southstar_netutil::effective_navigator_languages()
        .into_iter()
        .map(|lang| String::from_utf8_lossy(&lang).into_owned())
        .collect()
}

pub(crate) fn navigator_platform() -> String {
    southstar_net::navigator_platform().to_owned()
}

pub(crate) fn hint_platform() -> String {
    southstar_net::ua_hint_platform().to_owned()
}

pub(crate) fn mobile_mode() -> bool {
    southstar_net::is_mobile_mode()
}

pub(crate) struct UserAgentConfig {
    pub configured: Option<String>,
    pub compat_mode: Option<String>,
    pub do_not_track: bool,
    pub global_privacy_control: bool,
}

fn optional_text(text: &southstar_config::Text) -> Option<String> {
    text.bytes()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn user_agent_config() -> UserAgentConfig {
    match southstar_config::get() {
        Some(config) => UserAgentConfig {
            configured: optional_text(&config.user_agent).filter(|ua| !ua.is_empty()),
            compat_mode: optional_text(&config.compat_mode),
            do_not_track: config.do_not_track != 0,
            global_privacy_control: config.global_privacy_control != 0,
        },
        None => UserAgentConfig {
            configured: None,
            compat_mode: None,
            do_not_track: true,
            global_privacy_control: true,
        },
    }
}

pub(crate) fn user_agent_for_mode(compat_mode: Option<&str>) -> String {
    southstar_net::user_agent_for_mode(compat_mode.map(str::as_bytes)).to_owned()
}

pub(crate) fn has_client_hints(user_agent: &str) -> bool {
    southstar_net::has_client_hints(user_agent.as_bytes())
}

pub(crate) fn processor_count() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| u32::try_from(n.get()).unwrap_or(u32::MAX))
}

#[cfg(windows)]
pub(crate) fn physical_memory_bytes() -> Option<u64> {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> c_int;
    }
    let mut status = MemoryStatusEx {
        length: size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.total_phys)
}

#[cfg(not(windows))]
pub(crate) fn physical_memory_bytes() -> Option<u64> {
    None
}

macro_rules! export_native {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
                $f(scope, this, args)
            }
        )*
    };
}

export_native! {
    nsr_services_alert => console::alert,
    nsr_services_confirm => commands::confirm,
    nsr_services_prompt => commands::prompt,
    nsr_services_queue_microtask => window::queue_microtask,
    nsr_services_notification_ctor => window::notification,
    nsr_services_match_media => media::match_media,
    nsr_services_set_timeout => timers::set_timeout,
    nsr_services_set_interval => timers::set_interval,
    nsr_services_clear_timer => timers::clear,
    nsr_element_async_method => timers::element_async,
    nsr_cache_open => caches::open,
    nsr_services_request_idle_callback => timers::request_idle_callback,
    nsr_services_camera_request => camera::request,
    nsr_services_camera_release => camera::release,
    nsr_services_mic_release => camera::mic_release,
    nsr_services_camera_label => camera::label,
    nsr_services_camera_enumerate => camera::enumerate,
}

pub fn nsr_install_idle_deadline(scope: &mut Scope<'_>, global: &Value) {
    timers::install_idle_deadline_interface(scope, global);
}

pub fn nsr_services_install_console(scope: &mut Scope<'_>, global: &Value) {
    console::install(scope, global);
}

pub fn nsr_services_install_rtc(scope: &mut Scope<'_>, global: &Value) {
    rtc::install(scope, global);
}

pub fn nsr_services_install_clipboard_item(scope: &mut Scope<'_>, global: &Value) {
    clipboard_item::install(scope, global);
}

pub fn nsr_services_install_screen(scope: &mut Scope<'_>, global: &Value) {
    screen::install(scope, global);
}

pub fn timers_pending(js: Option<Page>, include_idle: bool) -> bool {
    timers::pending(Js(js), include_idle)
}

pub fn timer_count(js: Option<Page>) -> u32 {
    timers::count(Js(js)) as u32
}

pub fn purge_frame_timers(js: Option<Page>, frame: Node<'_>) {
    if let Some(frame) = frame.downgrade().get() {
        timers::purge_frame(Js(js), Frame(Some(frame)));
    }
}

pub fn timer_remove(js: Option<Page>, id: i32) {
    timers::remove(Js(js), id);
}

pub fn screen_size() -> (i32, i32) {
    let metrics = screen::metrics();
    (metrics.width, metrics.height)
}

pub fn reeval_media_queries(js: Option<Page>) {
    media::reevaluate(Js(js));
}

pub fn reset_page(js: Option<Page>) {
    crate::reset(Js(js));
}

pub fn nsr_services_window_navigator(scope: &mut Scope<'_>) -> Value {
    navigator::window(scope)
}

pub fn nsr_services_worker_navigator(scope: &mut Scope<'_>) -> Value {
    navigator::worker(scope)
}

pub fn nsr_services_console_emit(
    js: Option<Page>,
    scope: &mut Scope<'_>,
    prefix: &str,
    args: &[Value],
) {
    console::emit(scope, Js(js), prefix, args);
}

fn run_due_timers(js: Option<Page>) {
    timers::run_due(Js(js));
}

fn teardown(js: Option<Page>) {
    crate::teardown(Js(js));
}

pub fn install_runtime_hooks() {
    southstar_js_runtime::api::set_services_hooks(southstar_js_runtime::api::ServicesHooks {
        chrome_compat: navigator::chrome_compat,
        run_due_timers: |js| run_due_timers(Some(js)),
        purge_frame_timers,
        teardown: |js| teardown(Some(js)),
    });
    southstar_js_runtime::links::set_js_services_links(
        southstar_js_runtime::links::JsServicesLinks {
            cache_open: crate::ffi::nsr_cache_open,
            services_notification_ctor: crate::ffi::nsr_services_notification_ctor,
            services_queue_microtask: crate::ffi::nsr_services_queue_microtask,
            services_request_idle_callback: crate::ffi::nsr_services_request_idle_callback,
            services_match_media: crate::ffi::nsr_services_match_media,
            services_prompt: crate::ffi::nsr_services_prompt,
            services_confirm: crate::ffi::nsr_services_confirm,
            services_camera_enumerate: crate::ffi::nsr_services_camera_enumerate,
            services_camera_label: crate::ffi::nsr_services_camera_label,
            services_mic_release: crate::ffi::nsr_services_mic_release,
            services_camera_release: crate::ffi::nsr_services_camera_release,
            services_camera_request: crate::ffi::nsr_services_camera_request,
            services_clear_timer: crate::ffi::nsr_services_clear_timer,
            services_set_interval: crate::ffi::nsr_services_set_interval,
            services_set_timeout: crate::ffi::nsr_services_set_timeout,
            services_alert: crate::ffi::nsr_services_alert,
            services_worker_navigator: crate::ffi::nsr_services_worker_navigator,
            services_window_navigator: crate::ffi::nsr_services_window_navigator,
            install_idle_deadline: crate::ffi::nsr_install_idle_deadline,
            services_install_rtc: crate::ffi::nsr_services_install_rtc,
            services_install_screen: crate::ffi::nsr_services_install_screen,
            services_install_clipboard_item: crate::ffi::nsr_services_install_clipboard_item,
            services_install_console: crate::ffi::nsr_services_install_console,
        },
    );
}
