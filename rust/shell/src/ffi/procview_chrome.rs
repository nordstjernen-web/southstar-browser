//! Southstar — the tab view's own chrome: the context menu and its actions, the find bar, the permission bar, printing, saving the page and the developer tools window.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use core::sync::atomic::{AtomicI32, Ordering};

use southstar_glib::{self as glib, GBoolean, GError, GPtrArray};
use southstar_gtk_ffi::{self as gtk, GtkWidget};

use super::gtk::{connect, cstring, handler, set_accessible_label, tr};
use super::procview_input::{push_req, run_edit_op, start_select};
use super::procview_result::{
    ns_proc_view_back, ns_proc_view_can_back, ns_proc_view_can_forward, ns_proc_view_forward,
    ns_proc_view_load, ns_proc_view_reload, perm_resolve, post_emit,
};
use super::procview_state::{NsProcView, PrintSetup, Req};
use super::procview_view::{ns_proc_view_ref, ns_proc_view_unref};
use crate::procview::input::EditOp;

const EVT_STATUS: c_int = 2;
const EVT_NEWTAB: c_int = 3;

const REQ_FIND: c_int = 9;
const REQ_EXPORT: c_int = 10;
const REQ_CONSOLE: c_int = 11;
const REQ_EVAL: c_int = 12;
const REQ_DUMP: c_int = 13;
const REQ_COLOR_SCHEME: c_int = 18;
const REQ_PRINT: c_int = 22;

const EDIT_FIELD: c_int = 1;
const EDIT_WRITABLE: c_int = 2;
const EDIT_SELECTION: c_int = 4;

const DEV_TAB_CONSOLE: c_int = 0;
const DEV_TAB_NETWORK: c_int = 1;
const DEV_TAB_PERFORMANCE: c_int = 2;
const DEV_TAB_LAYOUT: c_int = 3;
const DEV_TAB_ELEMENTS: c_int = 4;

const CONSOLE_POLL_MS: c_uint = 250;

static EXPORT_COUNTER: AtomicI32 = AtomicI32::new(0);

unsafe extern "C" {
    fn g_malloc0(n_bytes: usize) -> *mut c_void;
    fn g_simple_action_group_new() -> *mut c_void;
    fn g_get_user_runtime_dir() -> *const c_char;
    fn g_strescape(source: *const c_char, exceptions: *const c_char) -> *mut c_char;
}

fn view<'a>(v: *mut NsProcView) -> &'a mut NsProcView {
    unsafe { &mut *v }
}

fn nonempty(s: *const c_char) -> bool {
    !s.is_null() && unsafe { *s } != 0
}

fn new_req(kind: c_int) -> *mut Req {
    let req = unsafe { g_malloc0(core::mem::size_of::<Req>()) }.cast::<Req>();
    unsafe { (*req).r#type = kind };
    req
}

fn is_window(widget: *mut GtkWidget) -> bool {
    !widget.is_null()
        && unsafe { gtk::g_type_check_instance_is_a(widget.cast(), gtk::gtk_window_get_type()) }
            != 0
}

fn page_title(v: *mut NsProcView) -> &'static [u8] {
    let title = view(v).current_title;
    if nonempty(title) {
        unsafe { CStr::from_ptr(title) }.to_bytes()
    } else {
        b"page"
    }
}

struct PrintJob {
    pages: *mut GPtrArray,
    setup: PrintSetup,
    raster_scale: f64,
}

unsafe extern "C" fn print_job_free(data: *mut c_void) {
    let job = unsafe { Box::from_raw(data.cast::<PrintJob>()) };
    unsafe {
        let pages = &*job.pages;
        for i in 0..pages.len as usize {
            gtk::cairo_surface_destroy((*pages.pdata.add(i)).cast());
        }
        glib::g_ptr_array_free(job.pages, 1);
    }
}

unsafe extern "C" fn on_print_begin(
    op: *mut gtk::GtkPrintOperation,
    _ctx: *mut c_void,
    data: *mut c_void,
) {
    let job = unsafe { &*data.cast::<PrintJob>() };
    unsafe { gtk::gtk_print_operation_set_n_pages(op, (*job.pages).len as c_int) };
}

unsafe extern "C" fn on_print_draw(
    _op: *mut c_void,
    ctx: *mut gtk::GtkPrintContext,
    page_nr: c_int,
    data: *mut c_void,
) {
    let job = unsafe { &*data.cast::<PrintJob>() };
    let pages = unsafe { &*job.pages };
    if page_nr < 0 || page_nr as c_uint >= pages.len {
        return;
    }
    unsafe {
        let cr = gtk::gtk_print_context_get_cairo_context(ctx);
        let surface_w = gtk::gtk_print_context_get_width(ctx);
        let mut scale = if job.setup.width > 0.0 {
            surface_w / job.setup.width
        } else {
            1.0
        };
        scale /= if job.raster_scale > 0.0 {
            job.raster_scale
        } else {
            1.0
        };
        gtk::cairo_save(cr);
        gtk::cairo_scale(cr, scale, scale);
        gtk::cairo_set_source_surface(cr, (*pages.pdata.add(page_nr as usize)).cast(), 0.0, 0.0);
        gtk::cairo_paint(cr);
        gtk::cairo_restore(cr);
    }
}

pub(super) fn print_run(
    v: *mut NsProcView,
    pages: *mut GPtrArray,
    setup: &PrintSetup,
    raster_scale: f64,
) {
    let job = Box::into_raw(Box::new(PrintJob {
        pages,
        setup: *setup,
        raster_scale: if raster_scale > 0.0 {
            raster_scale
        } else {
            1.0
        },
    }));
    let title = cstring(page_title(v));
    unsafe {
        let page_setup = gtk::gtk_page_setup_new();
        let paper = gtk::gtk_paper_size_new_custom(
            c"southstar".as_ptr(),
            c"Southstar".as_ptr(),
            setup.width * 72.0 / 96.0,
            setup.height * 72.0 / 96.0,
            gtk::UNIT_POINTS,
        );
        gtk::gtk_page_setup_set_paper_size(page_setup, paper);
        gtk::gtk_page_setup_set_orientation(
            page_setup,
            if setup.width > setup.height {
                gtk::PAGE_ORIENTATION_LANDSCAPE
            } else {
                gtk::PAGE_ORIENTATION_PORTRAIT
            },
        );
        gtk::gtk_paper_size_free(paper);

        let op = gtk::gtk_print_operation_new();
        gtk::gtk_print_operation_set_default_page_setup(op, page_setup);
        gtk::gtk_print_operation_set_use_full_page(op, 1);
        gtk::gtk_print_operation_set_unit(op, gtk::UNIT_POINTS);
        gtk::gtk_print_operation_set_job_name(op, title.as_ptr());
        gtk::gtk_print_operation_set_embed_page_setup(op, 1);
        connect(op, c"begin-print", handler!(on_print_begin), job.cast());
        connect(op, c"draw-page", handler!(on_print_draw), job.cast());
        gtk::g_object_set_data_full(
            op.cast(),
            c"ns-print-job".as_ptr(),
            job.cast(),
            Some(print_job_free),
        );

        let area = view(v).area;
        let root = if area.is_null() {
            ptr::null_mut()
        } else {
            gtk::gtk_widget_get_root(area)
        };
        let mut err: *mut GError = ptr::null_mut();
        gtk::gtk_print_operation_run(
            op,
            gtk::PRINT_OPERATION_ACTION_PRINT_DIALOG,
            root,
            (&raw mut err).cast(),
        );
        if !err.is_null() {
            post_emit(v, EVT_STATUS, (*err).message);
            glib::g_error_free(err);
        }
        gtk::g_object_unref(page_setup.cast());
        gtk::g_object_unref(op.cast());
    }
}

struct ExportCtx {
    view: *mut NsProcView,
    pdf: bool,
}

unsafe extern "C" fn on_save_dialog_done(src: *mut c_void, res: *mut c_void, ud: *mut c_void) {
    let ctx = unsafe { Box::from_raw(ud.cast::<ExportCtx>()) };
    let v = ctx.view;
    let mut err: *mut GError = ptr::null_mut();
    let file = unsafe { gtk::gtk_file_dialog_save_finish(src.cast(), res, (&raw mut err).cast()) };
    if !file.is_null() {
        unsafe {
            let dest = gtk::g_file_get_path(file);
            if !dest.is_null() && view(v).opened != 0 {
                let counter = EXPORT_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
                let base = cstring(format!(
                    "southstar-export-{}-{}.{}",
                    gtk::g_get_monotonic_time(),
                    counter,
                    if ctx.pdf { "pdf" } else { "png" }
                ));
                let req = new_req(REQ_EXPORT);
                let mut parts = [
                    g_get_user_runtime_dir().cast_mut(),
                    base.as_ptr().cast_mut(),
                    ptr::null_mut(),
                ];
                (*req).url = glib::g_build_filenamev(parts.as_mut_ptr());
                (*req).export_dest = glib::g_strdup(dest);
                push_req(v, req);
            }
            glib::g_free(dest.cast());
            gtk::g_object_unref(file);
        }
    }
    if !err.is_null() {
        unsafe { glib::g_error_free(err) };
    }
    ns_proc_view_unref(v);
}

pub(super) fn view_save(v: *mut NsProcView, pdf: bool) {
    let state = view(v);
    if state.opened == 0 || state.area.is_null() {
        return;
    }
    let mut name = page_title(v).to_vec();
    name.extend_from_slice(if pdf { b".pdf" } else { b".png" });
    for b in &mut name {
        if *b == b'/' {
            *b = b'_';
        }
    }
    let name = cstring(name);
    unsafe {
        let root = gtk::gtk_widget_get_root(state.area);
        let dialog = gtk::gtk_file_dialog_new();
        let title = if pdf {
            tr(c"Save page as PDF")
        } else {
            tr(c"Save page as PNG")
        };
        gtk::gtk_file_dialog_set_title(dialog, title.as_ptr());
        gtk::gtk_file_dialog_set_initial_name(dialog, name.as_ptr());
        let ctx = Box::into_raw(Box::new(ExportCtx {
            view: ns_proc_view_ref(v),
            pdf,
        }));
        gtk::gtk_file_dialog_save(
            dialog,
            root,
            ptr::null_mut(),
            Some(on_save_dialog_done),
            ctx.cast(),
        );
        gtk::g_object_unref(dialog.cast());
    }
}

fn ctx_set_clipboard(v: *mut NsProcView, text: *const c_char, status: &CStr) {
    let area = view(v).area;
    if !nonempty(text) || area.is_null() {
        return;
    }
    unsafe { gtk::gdk_clipboard_set_text(gtk::gtk_widget_get_clipboard(area), text) };
    post_emit(v, EVT_STATUS, tr(status).as_ptr());
}

unsafe extern "C" fn on_ctx_back(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    ns_proc_view_back(ud.cast());
}

unsafe extern "C" fn on_ctx_forward(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    ns_proc_view_forward(ud.cast());
}

unsafe extern "C" fn on_ctx_reload(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    ns_proc_view_reload(ud.cast());
}

unsafe extern "C" fn on_ctx_copy_url(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    ctx_set_clipboard(v, view(v).current_url, c"Copied page address");
}

unsafe extern "C" fn on_ctx_open_newtab(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    if nonempty(view(v).ctx_link) {
        post_emit(v, EVT_NEWTAB, view(v).ctx_link);
    }
}

unsafe extern "C" fn on_ctx_open_link(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    if nonempty(view(v).ctx_link) {
        ns_proc_view_load(v, view(v).ctx_link);
    }
}

unsafe extern "C" fn on_ctx_copy_link(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    ctx_set_clipboard(v, view(v).ctx_link, c"Copied link address");
}

unsafe extern "C" fn on_ctx_cut(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    run_edit_op(ud.cast(), EditOp::Cut);
}

unsafe extern "C" fn on_ctx_copy_sel(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    run_edit_op(ud.cast(), EditOp::Copy);
}

unsafe extern "C" fn on_ctx_paste(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    run_edit_op(ud.cast(), EditOp::Paste);
}

unsafe extern "C" fn on_ctx_select_all(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    view(v).has_selection = 1;
    start_select(v, 3, 0, 0);
}

unsafe extern "C" fn on_ctx_save_pdf(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    view_save(ud.cast(), true);
}

unsafe extern "C" fn on_ctx_save_png(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    view_save(ud.cast(), false);
}

type ActionFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);

const CTX_ACTIONS: [(&CStr, ActionFn); 13] = [
    (c"back", on_ctx_back),
    (c"forward", on_ctx_forward),
    (c"reload", on_ctx_reload),
    (c"copy-url", on_ctx_copy_url),
    (c"open-link", on_ctx_open_link),
    (c"open-newtab", on_ctx_open_newtab),
    (c"copy-link", on_ctx_copy_link),
    (c"cut", on_ctx_cut),
    (c"copy-sel", on_ctx_copy_sel),
    (c"paste", on_ctx_paste),
    (c"select-all", on_ctx_select_all),
    (c"save-pdf", on_ctx_save_pdf),
    (c"save-png", on_ctx_save_png),
];

fn ctx_action_enable(v: *mut NsProcView, name: &CStr, on: bool) {
    unsafe {
        let act = gtk::g_action_map_lookup_action(view(v).ctx_actions, name.as_ptr());
        if !act.is_null() {
            gtk::g_simple_action_set_enabled(act, glib::boolean(on));
        }
    }
}

pub(super) fn ctx_install_actions(v: *mut NsProcView) {
    unsafe {
        let group = g_simple_action_group_new();
        view(v).ctx_actions = group;
        for (name, activate) in CTX_ACTIONS {
            let action = gtk::g_simple_action_new(name.as_ptr(), ptr::null());
            connect(action, c"activate", handler!(activate), v.cast());
            gtk::g_action_map_add_action(group, action);
            gtk::g_object_unref(action);
        }
        gtk::gtk_widget_insert_action_group(view(v).area, c"ctx".as_ptr(), group);
    }
}

unsafe extern "C" fn popover_present_idle(data: *mut c_void) -> GBoolean {
    let pop = data.cast::<GtkWidget>();
    unsafe {
        if gtk::gtk_widget_get_mapped(pop) != 0 {
            gtk::gtk_popover_present(pop);
        }
        gtk::g_object_unref(pop.cast());
    }
    gtk::SOURCE_REMOVE
}

unsafe extern "C" fn on_popover_menu_mapped(pop: *mut GtkWidget, _ud: *mut c_void) {
    unsafe { gtk::g_idle_add(popover_present_idle, gtk::g_object_ref(pop.cast())) };
}

pub(super) fn ns_popover_menu_fit(popover: *mut GtkWidget) {
    if popover.is_null()
        || unsafe { gtk::g_type_check_instance_is_a(popover.cast(), gtk::gtk_popover_get_type()) }
            == 0
    {
        return;
    }
    let on_mapped = handler!(on_popover_menu_mapped);
    unsafe { connect(popover, c"map", on_mapped, ptr::null_mut()) };
}

fn menu_section(menu: *mut c_void, items: &[(&CStr, &CStr)]) {
    unsafe {
        let section = gtk::g_menu_new();
        for (label, action) in items {
            gtk::g_menu_append(section, tr(label).as_ptr(), action.as_ptr());
        }
        gtk::g_menu_append_section(menu, ptr::null(), section);
        gtk::g_object_unref(section);
    }
}

fn ctx_field_menu(v: *mut NsProcView, edit: c_int) -> *mut c_void {
    let selected = edit & EDIT_SELECTION != 0;
    let writable = edit & EDIT_WRITABLE != 0;
    ctx_action_enable(v, c"cut", writable && selected);
    ctx_action_enable(v, c"copy-sel", selected);
    ctx_action_enable(v, c"paste", writable);
    let menu = unsafe { gtk::g_menu_new() };
    menu_section(
        menu,
        &[
            (c"Cut", c"ctx.cut"),
            (c"Copy", c"ctx.copy-sel"),
            (c"Paste", c"ctx.paste"),
        ],
    );
    menu_section(menu, &[(c"Select All", c"ctx.select-all")]);
    menu
}

fn ctx_page_menu(v: *mut NsProcView) -> *mut c_void {
    let has_link = !view(v).ctx_link.is_null();
    let has_selection = view(v).has_selection != 0;
    ctx_action_enable(v, c"back", ns_proc_view_can_back(v));
    ctx_action_enable(v, c"forward", ns_proc_view_can_forward(v));
    ctx_action_enable(v, c"open-link", has_link);
    ctx_action_enable(v, c"open-newtab", has_link);
    ctx_action_enable(v, c"copy-link", has_link);
    ctx_action_enable(v, c"copy-sel", has_selection);
    let menu = unsafe { gtk::g_menu_new() };
    if has_link {
        let mut items = vec![(c"Open Link", c"ctx.open-link")];
        if !southstar_renderer_host::single_process_enabled() {
            items.push((c"Open Link in New Tab", c"ctx.open-newtab"));
        }
        items.push((c"Copy Link Address", c"ctx.copy-link"));
        menu_section(menu, &items);
    }
    if has_selection {
        menu_section(menu, &[(c"Copy", c"ctx.copy-sel")]);
    }
    menu_section(
        menu,
        &[
            (c"Back", c"ctx.back"),
            (c"Forward", c"ctx.forward"),
            (c"Reload", c"ctx.reload"),
        ],
    );
    menu_section(
        menu,
        &[
            (c"Select All", c"ctx.select-all"),
            (c"Copy Page Address", c"ctx.copy-url"),
            (c"Save Page as PDF…", c"ctx.save-pdf"),
            (c"Save Page as Image…", c"ctx.save-png"),
        ],
    );
    menu
}

pub(super) fn ns_pv_show_context_menu(v: *mut NsProcView, href: *const c_char, edit: c_int) {
    let state = view(v);
    unsafe { glib::g_free(state.ctx_link.cast()) };
    state.ctx_link = if nonempty(href) {
        unsafe { glib::g_strdup(href) }
    } else {
        ptr::null_mut()
    };
    let menu = if edit & EDIT_FIELD != 0 {
        ctx_field_menu(v, edit)
    } else {
        ctx_page_menu(v)
    };
    let state = view(v);
    unsafe {
        if !state.ctx_popover.is_null() {
            gtk::gtk_widget_unparent(state.ctx_popover);
        }
        state.ctx_popover = gtk::gtk_popover_menu_new_from_model(menu);
        gtk::g_object_unref(menu);
        gtk::gtk_widget_set_parent(state.ctx_popover, state.area);
        ns_popover_menu_fit(state.ctx_popover);
        gtk::gtk_popover_set_has_arrow(state.ctx_popover, 0);
        let rect = gtk::GdkRectangle {
            x: state.ctx_x as c_int,
            y: state.ctx_y as c_int,
            width: 1,
            height: 1,
        };
        gtk::gtk_popover_set_pointing_to(state.ctx_popover, &rect);
        gtk::gtk_popover_popup(state.ctx_popover);
    }
}

pub(super) fn request_find(v: *mut NsProcView, direction: c_int) {
    let state = view(v);
    if state.opened == 0 || state.search_entry.is_null() {
        return;
    }
    let query = unsafe { gtk::gtk_editable_get_text(state.search_entry) };
    let req = new_req(REQ_FIND);
    state.find_seq += 1;
    unsafe {
        let r = &mut *req;
        r.seq = state.find_seq;
        r.query = glib::g_strdup(if query.is_null() { c"".as_ptr() } else { query });
        r.find_dir = direction;
        r.find_from_y = state.scroll_y;
        r.find_case = state.find_case;
    }
    push_req(v, req);
}

pub(super) fn search_open(v: *mut NsProcView) {
    let state = view(v);
    if state.search_revealer.is_null() {
        return;
    }
    unsafe {
        gtk::gtk_revealer_set_reveal_child(state.search_revealer, 1);
        gtk::gtk_widget_grab_focus(state.search_entry);
        gtk::gtk_editable_select_region(state.search_entry, 0, -1);
        if nonempty(gtk::gtk_editable_get_text(state.search_entry)) {
            request_find(v, 0);
        }
    }
}

fn search_close(v: *mut NsProcView) {
    let state = view(v);
    if state.search_revealer.is_null() {
        return;
    }
    unsafe {
        gtk::gtk_revealer_set_reveal_child(state.search_revealer, 0);
        gtk::gtk_label_set_text(state.search_label, c"".as_ptr());
    }
    let req = new_req(REQ_FIND);
    state.find_seq += 1;
    unsafe {
        (*req).seq = state.find_seq;
        (*req).query = glib::g_strdup(c"".as_ptr());
        (*req).find_from_y = state.scroll_y;
    }
    push_req(v, req);
    unsafe { gtk::gtk_widget_grab_focus(view(v).area) };
}

pub(super) fn ns_proc_view_find_open(v: *mut NsProcView) {
    search_open(v);
}

pub(super) fn ns_proc_view_find_close(v: *mut NsProcView) -> bool {
    if v.is_null()
        || view(v).search_revealer.is_null()
        || unsafe { gtk::gtk_revealer_get_reveal_child(view(v).search_revealer) } == 0
    {
        return false;
    }
    search_close(v);
    true
}

pub(super) fn ns_proc_view_set_color_scheme(v: *mut NsProcView, dark: bool) {
    if v.is_null() {
        return;
    }
    let req = new_req(REQ_COLOR_SCHEME);
    unsafe { (*req).mods = c_int::from(dark) };
    push_req(v, req);
}

unsafe extern "C" fn on_search_changed(_e: *mut GtkWidget, data: *mut c_void) {
    request_find(data.cast(), 0);
}

unsafe extern "C" fn on_search_next(_w: *mut GtkWidget, data: *mut c_void) {
    request_find(data.cast(), 1);
}

unsafe extern "C" fn on_search_prev(_w: *mut GtkWidget, data: *mut c_void) {
    request_find(data.cast(), 2);
}

unsafe extern "C" fn on_search_stop(_e: *mut GtkWidget, data: *mut c_void) {
    search_close(data.cast());
}

pub(super) fn build_search_bar(v: *mut NsProcView) {
    let data = v.cast::<c_void>();
    let state = view(v);
    unsafe {
        let bx = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 4);
        gtk::gtk_widget_add_css_class(bx, c"toolbar".as_ptr());

        state.search_entry = gtk::gtk_search_entry_new();
        gtk::gtk_widget_set_size_request(state.search_entry, 220, -1);
        set_accessible_label(state.search_entry, tr(c"Find in page"));
        connect(
            state.search_entry,
            c"search-changed",
            handler!(on_search_changed),
            data,
        );
        connect(
            state.search_entry,
            c"activate",
            handler!(on_search_next),
            data,
        );
        connect(
            state.search_entry,
            c"next-match",
            handler!(on_search_next),
            data,
        );
        connect(
            state.search_entry,
            c"previous-match",
            handler!(on_search_prev),
            data,
        );
        connect(
            state.search_entry,
            c"stop-search",
            handler!(on_search_stop),
            data,
        );

        state.search_label = gtk::gtk_label_new(c"".as_ptr());
        gtk::gtk_widget_set_size_request(state.search_label, 56, -1);

        let prev = gtk::gtk_button_new_from_icon_name(c"go-up-symbolic".as_ptr());
        let next = gtk::gtk_button_new_from_icon_name(c"go-down-symbolic".as_ptr());
        let close = gtk::gtk_button_new_from_icon_name(c"window-close-symbolic".as_ptr());
        gtk::gtk_widget_set_tooltip_text(prev, tr(c"Previous match (Shift+Enter)").as_ptr());
        gtk::gtk_widget_set_tooltip_text(next, tr(c"Next match (Enter)").as_ptr());
        gtk::gtk_widget_set_tooltip_text(close, tr(c"Close (Esc)").as_ptr());
        set_accessible_label(prev, tr(c"Previous match"));
        set_accessible_label(next, tr(c"Next match"));
        set_accessible_label(close, tr(c"Close search"));
        connect(prev, c"clicked", handler!(on_search_prev), data);
        connect(next, c"clicked", handler!(on_search_next), data);
        connect(close, c"clicked", handler!(on_search_stop), data);

        gtk::gtk_box_append(bx, state.search_entry);
        gtk::gtk_box_append(bx, state.search_label);
        gtk::gtk_box_append(bx, prev);
        gtk::gtk_box_append(bx, next);
        gtk::gtk_box_append(bx, close);

        let frame = gtk::gtk_frame_new(ptr::null());
        gtk::gtk_widget_add_css_class(frame, c"ns-findbar".as_ptr());
        gtk::gtk_frame_set_child(frame, bx);

        state.search_revealer = gtk::gtk_revealer_new();
        gtk::gtk_revealer_set_transition_type(
            state.search_revealer,
            gtk::REVEALER_TRANSITION_TYPE_SLIDE_DOWN,
        );
        gtk::gtk_revealer_set_child(state.search_revealer, frame);
        gtk::gtk_widget_set_halign(state.search_revealer, gtk::ALIGN_END);
        gtk::gtk_widget_set_valign(state.search_revealer, gtk::ALIGN_START);
        gtk::gtk_widget_set_margin_top(state.search_revealer, 8);
        gtk::gtk_widget_set_margin_end(state.search_revealer, 20);
        gtk::gtk_overlay_add_overlay(state.overlay, state.search_revealer);
    }
}

unsafe extern "C" fn on_perm_allow(_btn: *mut GtkWidget, data: *mut c_void) {
    perm_resolve(data.cast(), true);
}

unsafe extern "C" fn on_perm_deny(_btn: *mut GtkWidget, data: *mut c_void) {
    perm_resolve(data.cast(), false);
}

pub(super) fn build_perm_bar(v: *mut NsProcView) {
    let data = v.cast::<c_void>();
    let state = view(v);
    unsafe {
        let bar = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 8);
        gtk::gtk_widget_set_margin_top(bar, 4);
        gtk::gtk_widget_set_margin_bottom(bar, 4);
        gtk::gtk_widget_set_margin_start(bar, 8);
        gtk::gtk_widget_set_margin_end(bar, 8);

        let icon = gtk::gtk_image_new_from_icon_name(c"dialog-question-symbolic".as_ptr());

        state.perm_label = gtk::gtk_label_new(c"".as_ptr());
        gtk::gtk_label_set_xalign(state.perm_label, 0.0);
        gtk::gtk_label_set_ellipsize(state.perm_label, gtk::ELLIPSIZE_MIDDLE);
        gtk::gtk_widget_set_hexpand(state.perm_label, 1);

        let allow = gtk::gtk_button_new_with_label(tr(c"Allow").as_ptr());
        gtk::gtk_widget_add_css_class(allow, c"suggested-action".as_ptr());
        gtk::gtk_widget_set_tooltip_text(allow, tr(c"Allow and trust this site").as_ptr());
        let deny = gtk::gtk_button_new_with_label(tr(c"Not now").as_ptr());
        let close = gtk::gtk_button_new_from_icon_name(c"window-close-symbolic".as_ptr());
        gtk::gtk_button_set_has_frame(close, 0);
        set_accessible_label(close, tr(c"Dismiss"));
        connect(allow, c"clicked", handler!(on_perm_allow), data);
        connect(deny, c"clicked", handler!(on_perm_deny), data);
        connect(close, c"clicked", handler!(on_perm_deny), data);

        gtk::gtk_box_append(bar, icon);
        gtk::gtk_box_append(bar, state.perm_label);
        gtk::gtk_box_append(bar, allow);
        gtk::gtk_box_append(bar, deny);
        gtk::gtk_box_append(bar, close);

        let frame = gtk::gtk_frame_new(ptr::null());
        gtk::gtk_frame_set_child(frame, bar);

        state.perm_revealer = gtk::gtk_revealer_new();
        gtk::gtk_revealer_set_transition_type(
            state.perm_revealer,
            gtk::REVEALER_TRANSITION_TYPE_SLIDE_DOWN,
        );
        gtk::gtk_revealer_set_child(state.perm_revealer, frame);
    }
}

pub(super) fn ns_pv_console_append(v: *mut NsProcView, text: *const c_char) {
    let state = view(v);
    if state.console_buffer.is_null() || !nonempty(text) {
        return;
    }
    unsafe {
        let mut end: gtk::GtkTextIter = core::mem::zeroed();
        gtk::gtk_text_buffer_get_end_iter(state.console_buffer, &mut end);
        gtk::gtk_text_buffer_insert(state.console_buffer, &mut end, text, -1);
        if !state.console_view.is_null() {
            gtk::gtk_text_buffer_get_end_iter(state.console_buffer, &mut end);
            let mark = gtk::gtk_text_buffer_create_mark(state.console_buffer, ptr::null(), &end, 0);
            gtk::gtk_text_view_scroll_mark_onscreen(state.console_view, mark);
            gtk::gtk_text_buffer_delete_mark(state.console_buffer, mark);
        }
    }
}

unsafe extern "C" fn console_poll_cb(data: *mut c_void) -> GBoolean {
    let v = data.cast::<NsProcView>();
    if view(v).console_open != 0 && view(v).opened != 0 {
        push_req(v, new_req(REQ_CONSOLE));
    }
    gtk::SOURCE_CONTINUE
}

unsafe extern "C" fn on_console_eval(entry: *mut GtkWidget, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let src = unsafe { gtk::gtk_editable_get_text(entry) };
    if !nonempty(src) || view(v).opened == 0 {
        return;
    }
    let mut echo = b"> ".to_vec();
    echo.extend_from_slice(unsafe { CStr::from_ptr(src) }.to_bytes());
    echo.push(b'\n');
    let echo = cstring(echo);
    ns_pv_console_append(v, echo.as_ptr());
    let req = new_req(REQ_EVAL);
    unsafe { (*req).query = glib::g_strdup(src) };
    push_req(v, req);
    unsafe { gtk::gtk_editable_set_text(entry, c"".as_ptr()) };
}

pub(super) fn ns_pv_console_set_open(v: *mut NsProcView, open: bool) {
    if open && view(v).console_window.is_null() {
        build_console_window(v);
    }
    let state = view(v);
    if state.console_window.is_null() {
        return;
    }
    state.console_open = GBoolean::from(open);
    unsafe {
        if open {
            let root = gtk::gtk_widget_get_root(state.area);
            if is_window(root) {
                gtk::gtk_window_set_transient_for(state.console_window, root);
            }
            gtk::gtk_window_present(state.console_window);
            if state.console_poll_id == 0 {
                state.console_poll_id =
                    gtk::g_timeout_add(CONSOLE_POLL_MS, console_poll_cb, v.cast());
            }
            gtk::gtk_widget_grab_focus(state.console_entry);
        } else {
            gtk::gtk_widget_set_visible(state.console_window, 0);
            if state.console_poll_id != 0 {
                gtk::g_source_remove(state.console_poll_id);
                state.console_poll_id = 0;
            }
            if !state.area.is_null() {
                gtk::gtk_widget_grab_focus(state.area);
            }
        }
    }
}

fn console_request_dump(v: *mut NsProcView, tab: c_int) {
    let kind = match tab {
        DEV_TAB_NETWORK => c"network",
        DEV_TAB_PERFORMANCE => c"performance",
        DEV_TAB_LAYOUT => c"layout",
        DEV_TAB_ELEMENTS => c"dom",
        _ => return,
    };
    if view(v).opened == 0 {
        return;
    }
    let req = new_req(REQ_DUMP);
    unsafe {
        (*req).dump_tab = tab;
        (*req).query = glib::g_strdup(kind.as_ptr());
    }
    push_req(v, req);
}

fn console_current_tab(v: *mut NsProcView) -> c_int {
    let notebook = view(v).console_notebook;
    if notebook.is_null() {
        DEV_TAB_CONSOLE
    } else {
        unsafe { gtk::gtk_notebook_get_current_page(notebook) }
    }
}

unsafe extern "C" fn on_console_clear(_b: *mut GtkWidget, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let state = view(v);
    let buf = match console_current_tab(v) {
        DEV_TAB_CONSOLE => state.console_buffer,
        DEV_TAB_NETWORK => state.net_buffer,
        DEV_TAB_PERFORMANCE => state.perf_buffer,
        DEV_TAB_LAYOUT => state.layout_buffer,
        DEV_TAB_ELEMENTS => state.elements_buffer,
        _ => ptr::null_mut(),
    };
    if !buf.is_null() {
        unsafe { gtk::gtk_text_buffer_set_text(buf, c"".as_ptr(), 0) };
    }
}

unsafe extern "C" fn on_console_refresh(_b: *mut GtkWidget, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    console_request_dump(v, console_current_tab(v));
}

unsafe extern "C" fn on_console_notebook_switch(
    _nb: *mut GtkWidget,
    _page: *mut GtkWidget,
    num: c_uint,
    data: *mut c_void,
) {
    console_request_dump(data.cast(), num as c_int);
}

const INSPECT_HEAD: &str = r#"(function(){try{var e=document.querySelector(""#;
const INSPECT_TAIL: &str = r#"");if(!e)return "";var s=e.outerHTML;return s.length>20000?s.slice(0,20000)+"\n…(truncated)":s;}catch(err){return "Error: "+err;}})()"#;

unsafe extern "C" fn on_inspect_activate(entry: *mut GtkWidget, data: *mut c_void) {
    let v = data.cast::<NsProcView>();
    let sel = unsafe { gtk::gtk_editable_get_text(entry) };
    if !nonempty(sel) || view(v).opened == 0 {
        return;
    }
    let mut src = INSPECT_HEAD.as_bytes().to_vec();
    unsafe {
        let esc = g_strescape(sel, ptr::null());
        src.extend_from_slice(CStr::from_ptr(esc).to_bytes());
        glib::g_free(esc.cast());
    }
    src.extend_from_slice(INSPECT_TAIL.as_bytes());
    let req = new_req(REQ_EVAL);
    unsafe {
        (*req).inspect = 1;
        (*req).dump_tab = DEV_TAB_ELEMENTS;
        (*req).query = glib::strdup(&src);
    }
    push_req(v, req);
}

unsafe extern "C" fn on_console_close_request(_win: *mut GtkWidget, data: *mut c_void) -> GBoolean {
    ns_pv_console_set_open(data.cast(), false);
    1
}

fn console_make_view(wrap: bool) -> (*mut GtkWidget, *mut GtkWidget, *mut c_void) {
    unsafe {
        let text = gtk::gtk_text_view_new();
        gtk::gtk_text_view_set_editable(text, 0);
        gtk::gtk_text_view_set_monospace(text, 1);
        gtk::gtk_text_view_set_cursor_visible(text, 0);
        gtk::gtk_text_view_set_wrap_mode(
            text,
            if wrap {
                gtk::WRAP_WORD_CHAR
            } else {
                gtk::WRAP_NONE
            },
        );
        let buffer = gtk::gtk_text_view_get_buffer(text);
        let scroll = gtk::gtk_scrolled_window_new();
        gtk::gtk_scrolled_window_set_child(scroll, text);
        gtk::gtk_widget_set_vexpand(scroll, 1);
        (scroll, text, buffer)
    }
}

fn console_add_tab(v: *mut NsProcView, child: *mut GtkWidget, title: &CStr) {
    unsafe {
        gtk::gtk_notebook_append_page(
            view(v).console_notebook,
            child,
            gtk::gtk_label_new(tr(title).as_ptr()),
        );
    }
}

fn build_console_window(v: *mut NsProcView) {
    let data = v.cast::<c_void>();
    let state = view(v);
    unsafe {
        let bx = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 0);

        let header = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 4);
        gtk::gtk_widget_add_css_class(header, c"toolbar".as_ptr());
        let spacer = gtk::gtk_label_new(c"".as_ptr());
        gtk::gtk_widget_set_hexpand(spacer, 1);
        let refresh = gtk::gtk_button_new_from_icon_name(c"view-refresh-symbolic".as_ptr());
        gtk::gtk_widget_set_tooltip_text(refresh, tr(c"Refresh").as_ptr());
        set_accessible_label(refresh, tr(c"Refresh"));
        connect(refresh, c"clicked", handler!(on_console_refresh), data);
        let clear = gtk::gtk_button_new_from_icon_name(c"edit-clear-symbolic".as_ptr());
        gtk::gtk_widget_set_tooltip_text(clear, tr(c"Clear").as_ptr());
        set_accessible_label(clear, tr(c"Clear"));
        connect(clear, c"clicked", handler!(on_console_clear), data);
        gtk::gtk_box_append(header, spacer);
        gtk::gtk_box_append(header, refresh);
        gtk::gtk_box_append(header, clear);

        state.console_notebook = gtk::gtk_notebook_new();
        gtk::gtk_widget_set_vexpand(state.console_notebook, 1);

        let console_page = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 0);
        let (console_scroll, console_view, console_buffer) = console_make_view(true);
        state.console_view = console_view;
        state.console_buffer = console_buffer;
        state.console_entry = gtk::gtk_entry_new();
        gtk::gtk_entry_set_placeholder_text(
            state.console_entry,
            tr(c"Evaluate JavaScript and press Enter").as_ptr(),
        );
        connect(
            state.console_entry,
            c"activate",
            handler!(on_console_eval),
            data,
        );
        gtk::gtk_box_append(console_page, console_scroll);
        gtk::gtk_box_append(console_page, state.console_entry);
        console_add_tab(v, console_page, c"Console");

        let (net_scroll, net_view, net_buffer) = console_make_view(false);
        state.net_view = net_view;
        state.net_buffer = net_buffer;
        console_add_tab(v, net_scroll, c"Network");
        let (perf_scroll, perf_view, perf_buffer) = console_make_view(false);
        state.perf_view = perf_view;
        state.perf_buffer = perf_buffer;
        console_add_tab(v, perf_scroll, c"Performance");
        let (layout_scroll, layout_view, layout_buffer) = console_make_view(false);
        state.layout_view = layout_view;
        state.layout_buffer = layout_buffer;
        console_add_tab(v, layout_scroll, c"Layout");

        let elements_page = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 0);
        let (elements_scroll, elements_view, elements_buffer) = console_make_view(false);
        state.elements_view = elements_view;
        state.elements_buffer = elements_buffer;
        state.inspect_entry = gtk::gtk_entry_new();
        gtk::gtk_entry_set_placeholder_text(
            state.inspect_entry,
            tr(c"Inspect: CSS selector, then Enter").as_ptr(),
        );
        connect(
            state.inspect_entry,
            c"activate",
            handler!(on_inspect_activate),
            data,
        );
        gtk::gtk_box_append(elements_page, elements_scroll);
        gtk::gtk_box_append(elements_page, state.inspect_entry);
        console_add_tab(v, elements_page, c"Elements");

        connect(
            state.console_notebook,
            c"switch-page",
            handler!(on_console_notebook_switch),
            data,
        );

        gtk::gtk_box_append(bx, header);
        gtk::gtk_box_append(bx, state.console_notebook);

        state.console_window = gtk::gtk_window_new();
        gtk::gtk_window_set_title(state.console_window, tr(c"Developer Tools").as_ptr());
        gtk::gtk_window_set_default_size(state.console_window, 720, 420);
        gtk::gtk_window_set_destroy_with_parent(state.console_window, 1);
        gtk::gtk_window_set_child(state.console_window, bx);
        connect(
            state.console_window,
            c"close-request",
            handler!(on_console_close_request),
            data,
        );
    }
}

pub(super) fn ns_proc_view_print(v: *mut NsProcView) {
    if v.is_null() || view(v).opened == 0 {
        return;
    }
    push_req(v, new_req(REQ_PRINT));
}

pub(super) fn ns_proc_view_save_pdf(v: *mut NsProcView) {
    if !v.is_null() {
        view_save(v, true);
    }
}

pub(super) fn ns_proc_view_save_image(v: *mut NsProcView) {
    if !v.is_null() {
        view_save(v, false);
    }
}
