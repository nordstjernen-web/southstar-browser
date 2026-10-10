//! Southstar — the C ABI of the window services as declared in src/js_internal.h, and the js.c, network and system calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::screen::{self, Metrics};
use crate::{clipboard_item, console, media, navigator, rtc, timers, window};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
struct TimerScope {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(*mut JSContext);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Frame(*const c_void);

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
    js: *const NsJs,
    id: i32,
}

const TIMER_WAIT: c_int = 1;
const TIMER_DROP: c_int = 2;
const G_PRIORITY_DEFAULT: c_int = 0;

type SourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn ns_js_due_timers_allowed(js: *const NsJs) -> GBoolean;
    fn ns_js_timer_gate(js: *const NsJs, frame: *const c_void, idle_expired: GBoolean) -> c_int;
    fn ns_js_timer_scope_enter(
        js: *const NsJs,
        ctx: *mut JSContext,
        frame: *const c_void,
    ) -> *mut TimerScope;
    fn ns_js_timer_scope_context(scope: *const TimerScope) -> *mut JSContext;
    fn ns_js_timer_scope_leave(scope: *mut TimerScope, threw: GBoolean, exception: JSValue);
    fn ns_timer_this_is_detached_window(
        js: *const NsJs,
        ctx: *mut JSContext,
        this_val: JSValue,
    ) -> GBoolean;
    fn ns_js_context_frame(js: *const NsJs, ctx: *mut JSContext) -> *const c_void;
    fn ns_js_idle_frame_end(js: *const NsJs, now: i64, end: i64) -> i64;
    fn ns_js_source_remove(js: *const NsJs, id: c_uint);
    fn ns_js_glib_context(js: *const NsJs) -> *mut c_void;
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
    Realm(quickjs::raw_context(scope))
}

pub(crate) fn context_frame(scope: &Scope<'_>, js: Js) -> Frame {
    Frame(unsafe { ns_js_context_frame(js.0, quickjs::raw_context(scope)) })
}

pub(crate) fn this_is_detached_window(scope: &Scope<'_>, js: Js, this: &Value) -> bool {
    unsafe {
        ns_timer_this_is_detached_window(js.0, quickjs::raw_context(scope), quickjs::raw(this)) != 0
    }
}

pub(crate) fn idle_frame_end(js: Js, now: i64, end: i64) -> i64 {
    unsafe { ns_js_idle_frame_end(js.0, now, end) }
}

pub(crate) fn due_timers_allowed(js: Js) -> bool {
    unsafe { ns_js_due_timers_allowed(js.0) != 0 }
}

pub(crate) fn timer_gate(js: Js, frame: Frame, idle_expired: bool) -> Gate {
    match unsafe { ns_js_timer_gate(js.0, frame.0, glib::boolean(idle_expired)) } {
        TIMER_WAIT => Gate::Wait,
        TIMER_DROP => Gate::Drop,
        _ => Gate::Run,
    }
}

pub(crate) fn in_timer_scope(
    js: Js,
    realm: Realm,
    frame: Frame,
    f: impl FnOnce(&mut Scope<'_>) -> Result<Value, Value>,
) {
    let timer_scope = unsafe { ns_js_timer_scope_enter(js.0, realm.0, frame.0) };
    let ctx = unsafe { ns_js_timer_scope_context(timer_scope) };
    let result = unsafe { quickjs::with_context(ctx, f) };
    let (threw, exception) = match &result {
        Ok(_) => (glib::FALSE, quickjs::UNDEFINED),
        Err(error) => (glib::TRUE, quickjs::raw(error)),
    };
    unsafe { ns_js_timer_scope_leave(timer_scope, threw, exception) };
    drop(result);
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
        let attached = g_source_attach(source, ns_js_glib_context(js.0));
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
        unsafe { ns_js_source_remove(js.0, source.0) };
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*const NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn key(self) -> usize {
        self.0 as usize
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

unsafe extern "C" {
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue);
    fn ns_target_dispatchEvent(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_navigator_sendBeacon(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_eme_request_access(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_media_capabilities_info(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_js_user_activation_state(js: *const NsJs, ever: *mut GBoolean) -> GBoolean;
    fn ns_js_clipboard_write(js: *const NsJs, text: *const c_char) -> c_int;
    fn ns_net_navigator_languages() -> *mut *mut c_char;
    fn ns_net_navigator_platform() -> *const c_char;
    fn ns_net_ua_hint_platform() -> *const c_char;
    fn ns_net_is_mobile_mode() -> GBoolean;
    fn ns_user_agent_for_mode(compat_mode: *const c_char) -> *const c_char;
    fn ns_user_agent_has_client_hints(user_agent: *const c_char) -> GBoolean;
    fn g_get_num_processors() -> c_uint;
    fn ns_js_log_enabled(js: *const NsJs) -> GBoolean;
    fn ns_js_log_line(js: *const NsJs, line: *const c_char);
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_target_fire_event(ctx: *mut JSContext, obj: JSValue, kind: *const c_char);
    fn ns_target_make_event(ctx: *mut JSContext, target: JSValue, kind: *const c_char) -> JSValue;
    fn ns_target_dispatch_with_event(
        ctx: *mut JSContext,
        obj: JSValue,
        kind: *const c_char,
        event: JSValue,
    );
    fn ns_event_adopt_interface(ctx: *mut JSContext, event: JSValue, iface: *const c_char);
    fn ns_css_media_query_matches(query: *const c_char) -> GBoolean;
    fn ns_css_media_list_serialize(query: *const c_char) -> *mut c_char;
    fn ns_css_set_device_size(width: f64, height: f64);
}

fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

fn c_ptr(text: &Option<CString>) -> *const c_char {
    text.as_ref()
        .map_or(core::ptr::null(), |text| text.as_ptr())
}

pub(crate) fn log_enabled(js: Js) -> bool {
    !js.is_null() && unsafe { ns_js_log_enabled(js.0) } != 0
}

pub(crate) fn log_line(js: Js, line: &str) {
    if js.is_null() {
        return;
    }
    let line = c_string(line);
    unsafe { ns_js_log_line(js.0, line.as_ptr()) };
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    let ctx = unsafe { ns_js_main_context(js.0) };
    if ctx.is_null() {
        return None;
    }
    Some(unsafe { quickjs::with_context(ctx, f) })
}

pub(crate) fn fire_event(scope: &mut Scope<'_>, target: &Value, kind: &str) {
    let kind = c_string(kind);
    unsafe {
        ns_target_fire_event(
            quickjs::raw_context(scope),
            quickjs::raw(target),
            kind.as_ptr(),
        )
    };
}

pub(crate) fn make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    let kind = c_string(kind);
    let raw = unsafe {
        ns_target_make_event(
            quickjs::raw_context(scope),
            quickjs::raw(target),
            kind.as_ptr(),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    let iface = c_string(iface);
    unsafe {
        ns_event_adopt_interface(
            quickjs::raw_context(scope),
            quickjs::raw(event),
            iface.as_ptr(),
        )
    };
}

pub(crate) fn dispatch_with_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    event: &Value,
) {
    let kind = c_string(kind);
    unsafe {
        ns_target_dispatch_with_event(
            quickjs::raw_context(scope),
            quickjs::raw(target),
            kind.as_ptr(),
            quickjs::raw(event),
        )
    };
}

pub(crate) fn media_query_matches(query: Option<&str>) -> bool {
    let query = query.map(c_string);
    unsafe { ns_css_media_query_matches(c_ptr(&query)) != 0 }
}

pub(crate) fn media_list_serialize(query: Option<&str>) -> String {
    let query = query.map(c_string);
    let serialized = unsafe { ns_css_media_list_serialize(c_ptr(&query)) };
    let out = text(serialized);
    if !serialized.is_null() {
        unsafe { glib::g_free(serialized.cast()) };
    }
    out
}

pub(crate) fn set_device_size(width: i32, height: i32) {
    unsafe { ns_css_set_device_size(f64::from(width), f64::from(height)) };
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
    Js(quickjs::context_opaque(scope).cast())
}

fn text(p: *const c_char) -> String {
    unsafe { glib::bytes(p) }
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    unsafe { ns_bind_event_target_listeners(quickjs::raw_context(scope), quickjs::raw(object)) };
}

pub(crate) fn c_method(scope: &mut Scope<'_>, name: &str, arity: u32, method: CMethod) -> Value {
    let f: quickjs::JSCFunction = match method {
        CMethod::DispatchEvent => ns_target_dispatchEvent,
        CMethod::SendBeacon => ns_navigator_sendBeacon,
        CMethod::EmeRequestAccess => ns_eme_request_access,
        CMethod::MediaCapabilitiesInfo => ns_media_capabilities_info,
    };
    quickjs::c_function(scope, name, arity, f)
}

pub(crate) fn user_activation(js: Js) -> (bool, bool) {
    if js.0.is_null() {
        return (false, false);
    }
    let mut ever = glib::FALSE;
    let transient = unsafe { ns_js_user_activation_state(js.0, &mut ever) };
    (transient != 0, ever != 0)
}

pub(crate) fn clipboard_write(js: Js, text: &str) -> Option<bool> {
    if js.0.is_null() {
        return None;
    }
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    match unsafe { ns_js_clipboard_write(js.0, text.as_ptr()) } {
        status if status < 0 => None,
        status => Some(status != 0),
    }
}

pub(crate) fn languages() -> Vec<String> {
    let list = unsafe { ns_net_navigator_languages() };
    let mut out = Vec::new();
    if list.is_null() {
        return out;
    }
    let mut index = 0;
    loop {
        let item = unsafe { *list.add(index) };
        if item.is_null() {
            break;
        }
        out.push(text(item));
        index += 1;
    }
    unsafe { glib::g_strfreev(list) };
    out
}

pub(crate) fn navigator_platform() -> String {
    text(unsafe { ns_net_navigator_platform() })
}

pub(crate) fn hint_platform() -> String {
    text(unsafe { ns_net_ua_hint_platform() })
}

pub(crate) fn mobile_mode() -> bool {
    unsafe { ns_net_is_mobile_mode() != 0 }
}

pub(crate) struct UserAgentConfig {
    pub configured: Option<String>,
    pub compat_mode: Option<String>,
    pub do_not_track: bool,
    pub global_privacy_control: bool,
}

fn optional_text(p: *const c_char) -> Option<String> {
    unsafe { glib::bytes(p) }.map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn user_agent_config() -> UserAgentConfig {
    match southstar_config::get() {
        Some(config) => UserAgentConfig {
            configured: optional_text(config.user_agent).filter(|ua| !ua.is_empty()),
            compat_mode: optional_text(config.compat_mode),
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
    let mode = compat_mode.and_then(|mode| CString::new(mode).ok());
    text(unsafe { ns_user_agent_for_mode(mode.as_ref().map_or(core::ptr::null(), |m| m.as_ptr())) })
}

pub(crate) fn has_client_hints(user_agent: &str) -> bool {
    let Ok(user_agent) = CString::new(user_agent) else {
        return false;
    };
    unsafe { ns_user_agent_has_client_hints(user_agent.as_ptr()) != 0 }
}

pub(crate) fn processor_count() -> u32 {
    unsafe { g_get_num_processors() }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_window_navigator(ctx: *mut JSContext) -> JSValue {
    unsafe { quickjs::with_context(ctx, |scope| quickjs::into_raw(navigator::window(scope))) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_worker_navigator(ctx: *mut JSContext) -> JSValue {
    unsafe { quickjs::with_context(ctx, |scope| quickjs::into_raw(navigator::worker(scope))) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_services_chrome_compat() -> GBoolean {
    glib::boolean(navigator::chrome_compat())
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

macro_rules! export_native {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

export_native! {
    ns_services_alert => console::alert,
    ns_services_queue_microtask => window::queue_microtask,
    ns_services_notification_ctor => window::notification,
    ns_services_match_media => media::match_media,
    ns_services_set_timeout => timers::set_timeout,
    ns_services_set_interval => timers::set_interval,
    ns_services_clear_timer => timers::clear,
    ns_services_request_idle_callback => timers::request_idle_callback,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_install_idle_deadline(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            timers::install_idle_deadline(scope, &proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_run_due_timers(js: *const NsJs) {
    timers::run_due(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_timers_pending(
    js: *const NsJs,
    include_idle: GBoolean,
) -> GBoolean {
    glib::boolean(timers::pending(Js(js), include_idle != 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_timer_count(js: *const NsJs) -> c_uint {
    timers::count(Js(js)) as c_uint
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_purge_frame_timers(js: *const NsJs, frame: *const c_void) {
    if !frame.is_null() {
        timers::purge_frame(Js(js), Frame(frame));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_timer_remove(js: *const NsJs, id: c_int) {
    timers::remove(Js(js), id);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_install_console(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            console::install(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_install_rtc(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            rtc::install(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_install_clipboard_item(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            clipboard_item::install(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_install_screen(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            screen::install(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_console_emit(
    js: *const NsJs,
    prefix: *const c_char,
    ctx: *mut JSContext,
    argc: c_int,
    argv: *mut JSValue,
) {
    let prefix = text(prefix);
    let raw_args = if argv.is_null() || argc <= 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let args: Vec<Value> = raw_args
                .iter()
                .map(|&raw| quickjs::borrow_value(scope, raw))
                .collect();
            console::emit(scope, Js(js), &prefix, &args);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_screen_metrics(
    width: *mut c_int,
    height: *mut c_int,
    avail_width: *mut c_int,
    avail_height: *mut c_int,
    avail_left: *mut c_int,
    avail_top: *mut c_int,
) {
    let Metrics {
        width: w,
        height: h,
        avail_width: aw,
        avail_height: ah,
        avail_left: al,
        avail_top: at,
    } = screen::metrics();
    for (out, value) in [
        (width, w),
        (height, h),
        (avail_width, aw),
        (avail_height, ah),
        (avail_left, al),
        (avail_top, at),
    ] {
        if !out.is_null() {
            unsafe { *out = value };
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_reeval_media_queries(js: *const NsJs) {
    media::reevaluate(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_reset(js: *const NsJs) {
    crate::reset(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_teardown(js: *const NsJs) {
    crate::teardown(Js(js));
}
