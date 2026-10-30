//! Southstar — the tab view's own chrome: the context menu and its actions, the find bar, the permission bar, printing, saving the page and the developer tools window.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::ops::ControlFlow;
use core::ptr;
use core::sync::atomic::{AtomicI32, Ordering};
use std::ffi::CString;

use southstar_cairo::Surface;
use southstar_glib::{self as glib, GBoolean};
use southstar_gtk::{
    self as gtk, Button, Entry, FileDialog, Frame, Label, Overlay, PageSetup, PrintContext,
    PrintOperation, Revealer, ScrolledWindow, SearchEntry, TextView, Widget, Window,
};
use southstar_gtk_ffi::{self as sys, GtkWidget};

use super::gtk::{build_filename, connect, cstring, handler, tr};
use super::procview_input::{area, new_req, push_req, run_edit_op, start_select, view};
use super::procview_result::{
    ns_proc_view_back, ns_proc_view_can_back, ns_proc_view_can_forward, ns_proc_view_forward,
    ns_proc_view_load, ns_proc_view_reload, ns_pv_page_may_open, post_emit,
};
use super::procview_state::{Console, FindBar, NsProcView, PermBar, PrintSetup};
use super::procview_view::{ns_proc_view_ref, ns_proc_view_unref};
use crate::procview::input::EditOp;
use crate::procview::nav;

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

const CONSOLE_POLL_MS: u32 = 250;

static EXPORT_COUNTER: AtomicI32 = AtomicI32::new(0);

unsafe extern "C" {
    fn g_simple_action_group_new() -> *mut c_void;
}

fn page_title(v: *mut NsProcView) -> Vec<u8> {
    match &view(v).ui().current_title {
        Some(title) if !title.is_empty() => title.to_bytes().to_vec(),
        _ => b"page".to_vec(),
    }
}

fn find_bar(v: *mut NsProcView) -> Option<(Revealer, SearchEntry, Label)> {
    let find = view(v).ui().find.as_ref()?;
    Some((
        find.revealer.clone(),
        find.entry.clone(),
        find.label.clone(),
    ))
}

fn console_window(v: *mut NsProcView) -> Option<(Window, Entry)> {
    let console = view(v).ui().console.as_ref()?;
    Some((console.window.clone(), console.entry.clone()))
}

fn console_text(v: *mut NsProcView, tab: c_int) -> Option<TextView> {
    let console = view(v).ui().console.as_ref()?;
    match tab {
        DEV_TAB_CONSOLE => Some(console.log.clone()),
        DEV_TAB_NETWORK => Some(console.network.clone()),
        DEV_TAB_PERFORMANCE => Some(console.performance.clone()),
        DEV_TAB_LAYOUT => Some(console.layout.clone()),
        DEV_TAB_ELEMENTS => Some(console.elements.clone()),
        _ => None,
    }
}

pub(super) fn ns_pv_console_text(v: *mut NsProcView, tab: c_int) -> Option<TextView> {
    console_text(v, tab)
}

struct PrintJob {
    pages: Vec<Surface>,
    setup: PrintSetup,
    raster_scale: f64,
}

fn draw_sheet(job: &PrintJob, context: &PrintContext<'_>, page_nr: c_int) {
    let Some(page) = usize::try_from(page_nr).ok().and_then(|i| job.pages.get(i)) else {
        return;
    };
    let Some(cr) = context.cairo_context() else {
        return;
    };
    let mut scale = if job.setup.width > 0.0 {
        context.width() / job.setup.width
    } else {
        1.0
    };
    scale /= if job.raster_scale > 0.0 {
        job.raster_scale
    } else {
        1.0
    };
    cr.save();
    cr.scale(scale, scale);
    cr.set_source_surface(page.as_ref(), 0.0, 0.0);
    cr.paint();
    cr.restore();
}

pub(super) fn print_run(
    v: *mut NsProcView,
    pages: Vec<Surface>,
    setup: &PrintSetup,
    raster_scale: f64,
) {
    let job = PrintJob {
        pages,
        setup: *setup,
        raster_scale: if raster_scale > 0.0 {
            raster_scale
        } else {
            1.0
        },
    };
    let n_pages = c_int::try_from(job.pages.len()).unwrap_or(c_int::MAX);
    let title = cstring(page_title(v));

    let page_setup = PageSetup::new();
    page_setup.set_custom_paper_size(
        c"southstar",
        c"Southstar",
        setup.width * 72.0 / 96.0,
        setup.height * 72.0 / 96.0,
        gtk::UNIT_POINTS,
    );
    page_setup.set_orientation(if setup.width > setup.height {
        gtk::PAGE_ORIENTATION_LANDSCAPE
    } else {
        gtk::PAGE_ORIENTATION_PORTRAIT
    });

    let op = PrintOperation::new();
    op.set_default_page_setup(&page_setup);
    op.set_use_full_page(true);
    op.set_unit(gtk::UNIT_POINTS);
    op.set_job_name(&title);
    op.set_embed_page_setup(true);
    op.connect_begin_print(move |op| op.set_n_pages(n_pages));
    op.connect_draw_page(move |context, page_nr| draw_sheet(&job, context, page_nr));

    let root = area(v).and_then(|area| area.root());
    if let Err(message) = op.run(gtk::PRINT_OPERATION_ACTION_PRINT_DIALOG, root.as_ref()) {
        post_emit(v, EVT_STATUS, Some(&message));
    }
}

fn export_to(v: *mut NsProcView, dest: &[u8], pdf: bool) {
    if view(v).opened == 0 {
        return;
    }
    let counter = EXPORT_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    let base = cstring(format!(
        "southstar-export-{}-{}.{}",
        glib::monotonic_us(),
        counter,
        if pdf { "pdf" } else { "png" }
    ));
    let exchange = cstring(southstar_renderer_client::exchange_dir());
    let file = build_filename(&[&exchange, &base]);
    push_req(
        v,
        new_req(REQ_EXPORT, |r| {
            r.url = glib::strdup(file.to_bytes());
            r.export_dest = glib::strdup(dest);
        }),
    );
}

pub(super) fn view_save(v: *mut NsProcView, pdf: bool) {
    if view(v).opened == 0 {
        return;
    }
    let Some(area) = area(v) else {
        return;
    };
    let mut name = page_title(v);
    name.extend_from_slice(if pdf { b".pdf" } else { b".png" });
    for b in &mut name {
        if *b == b'/' {
            *b = b'_';
        }
    }
    let dialog = FileDialog::new();
    dialog.set_title(if pdf {
        tr(c"Save page as PDF")
    } else {
        tr(c"Save page as PNG")
    });
    dialog.set_initial_name(&cstring(name));
    let held = ns_proc_view_ref(v);
    dialog.save(area.root().as_ref(), move |dest| {
        if let Some(dest) = dest {
            export_to(held, &dest, pdf);
        }
        ns_proc_view_unref(held);
    });
}

fn ctx_link(v: *mut NsProcView) -> Option<CString> {
    view(v).ui().ctx_link.clone()
}

fn ctx_set_clipboard(v: *mut NsProcView, text: Option<CString>, status: &CStr) {
    let (Some(text), Some(area)) = (text.filter(|t| !t.is_empty()), area(v)) else {
        return;
    };
    area.set_clipboard_text(&text);
    post_emit(v, EVT_STATUS, Some(tr(status)));
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
    let url = view(v).ui().current_url.clone();
    ctx_set_clipboard(v, url, c"Copied page address");
}

unsafe extern "C" fn on_ctx_open_newtab(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    if let Some(link) = ctx_link(v)
        && ns_pv_page_may_open(v, &link)
    {
        post_emit(v, EVT_NEWTAB, Some(&link));
    }
}

unsafe extern "C" fn on_ctx_open_link(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    if let Some(link) = ctx_link(v)
        && ns_pv_page_may_open(v, &link)
    {
        ns_proc_view_load(v, &link);
    }
}

unsafe extern "C" fn on_ctx_copy_link(_a: *mut c_void, _p: *mut c_void, ud: *mut c_void) {
    let v = ud.cast::<NsProcView>();
    ctx_set_clipboard(v, ctx_link(v), c"Copied link address");
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
    // SAFETY: ctx_actions is the view's live action group and name is NUL-terminated.
    let act = unsafe { sys::g_action_map_lookup_action(view(v).ctx_actions, name.as_ptr()) };
    if !act.is_null() {
        // SAFETY: act is a GSimpleAction of the group, alive while the group is.
        unsafe { sys::g_simple_action_set_enabled(act, glib::boolean(on)) };
    }
}

pub(super) fn ctx_install_actions(v: *mut NsProcView) {
    // SAFETY: g_simple_action_group_new has no preconditions; the view keeps the reference it returns.
    let group = unsafe { g_simple_action_group_new() };
    view(v).ctx_actions = group;
    for (name, activate) in CTX_ACTIONS {
        // SAFETY: name is NUL-terminated and a null parameter type makes a plain action.
        let action = unsafe { sys::g_simple_action_new(name.as_ptr(), ptr::null()) };
        let callback = handler!(activate);
        // SAFETY: action is live and activate takes (action, parameter, data) with the view as data, which outlives its actions.
        unsafe { connect(action, c"activate", callback, v.cast()) };
        // SAFETY: group and action are live; the group takes its own reference to the action.
        unsafe { sys::g_action_map_add_action(group, action) };
        // SAFETY: drops the reference g_simple_action_new returned.
        unsafe { sys::g_object_unref(action) };
    }
    // SAFETY: area is the view's live drawing area and group a live action group it references.
    unsafe { sys::gtk_widget_insert_action_group(view(v).area, c"ctx".as_ptr(), group) };
}

unsafe extern "C" fn popover_present_idle(data: *mut c_void) -> GBoolean {
    let pop = data.cast::<GtkWidget>();
    // SAFETY: data is the popover on_popover_menu_mapped holds a reference to.
    if unsafe { sys::gtk_widget_get_mapped(pop) } != 0 {
        // SAFETY: pop is a live, mapped GtkPopover.
        unsafe { sys::gtk_popover_present(pop) };
    }
    // SAFETY: drops the reference on_popover_menu_mapped took for this idle source.
    unsafe { sys::g_object_unref(pop.cast()) };
    sys::SOURCE_REMOVE
}

unsafe extern "C" fn on_popover_menu_mapped(pop: *mut GtkWidget, _ud: *mut c_void) {
    // SAFETY: GTK emits map on the live popover; the idle source owns the reference taken here.
    let held = unsafe { sys::g_object_ref(pop.cast()) };
    // SAFETY: popover_present_idle releases held exactly once.
    unsafe { sys::g_idle_add(popover_present_idle, held) };
}

pub(super) fn ns_popover_menu_fit(popover: *mut GtkWidget) {
    if popover.is_null() {
        return;
    }
    // SAFETY: the type getter registers the class if needed and has no preconditions.
    let popover_type = unsafe { sys::gtk_popover_get_type() };
    // SAFETY: the caller passes a live widget.
    if unsafe { sys::g_type_check_instance_is_a(popover.cast(), popover_type) } == 0 {
        return;
    }
    let on_mapped = handler!(on_popover_menu_mapped);
    // SAFETY: popover is live and map takes (widget, data), as on_popover_menu_mapped does.
    unsafe { connect(popover, c"map", on_mapped, ptr::null_mut()) };
}

fn new_menu() -> *mut c_void {
    // SAFETY: g_menu_new has no preconditions and returns a new menu the caller owns.
    unsafe { sys::g_menu_new() }
}

fn menu_section(menu: *mut c_void, items: &[(&CStr, &CStr)]) {
    let section = new_menu();
    for (label, action) in items {
        // SAFETY: section is the live menu above and both strings are NUL-terminated; GLib copies them.
        unsafe { sys::g_menu_append(section, tr(label).as_ptr(), action.as_ptr()) };
    }
    // SAFETY: menu and section are live menus; menu takes its own reference to section.
    unsafe { sys::g_menu_append_section(menu, ptr::null(), section) };
    // SAFETY: drops the reference g_menu_new returned.
    unsafe { sys::g_object_unref(section) };
}

fn ctx_field_menu(v: *mut NsProcView, edit: c_int) -> *mut c_void {
    let selected = edit & EDIT_SELECTION != 0;
    let writable = edit & EDIT_WRITABLE != 0;
    ctx_action_enable(v, c"cut", writable && selected);
    ctx_action_enable(v, c"copy-sel", selected);
    ctx_action_enable(v, c"paste", writable);
    let menu = new_menu();
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
    let has_link = view(v).ui().ctx_link.is_some();
    let has_selection = view(v).has_selection != 0;
    ctx_action_enable(v, c"back", ns_proc_view_can_back(v));
    ctx_action_enable(v, c"forward", ns_proc_view_can_forward(v));
    ctx_action_enable(v, c"open-link", has_link);
    ctx_action_enable(v, c"open-newtab", has_link);
    ctx_action_enable(v, c"copy-link", has_link);
    ctx_action_enable(v, c"copy-sel", has_selection);
    let menu = new_menu();
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

pub(super) fn ns_pv_show_context_menu(v: *mut NsProcView, href: Option<&CStr>, edit: c_int) {
    view(v).ui().ctx_link = href.filter(|h| !h.is_empty()).map(CStr::to_owned);
    let menu = if edit & EDIT_FIELD != 0 {
        ctx_field_menu(v, edit)
    } else {
        ctx_page_menu(v)
    };
    let state = view(v);
    if !state.ctx_popover.is_null() {
        // SAFETY: ctx_popover is the live popover parented to the area.
        unsafe { sys::gtk_widget_unparent(state.ctx_popover) };
    }
    // SAFETY: menu is a live menu model; the popover takes its own reference.
    state.ctx_popover = unsafe { sys::gtk_popover_menu_new_from_model(menu) };
    // SAFETY: drops the reference g_menu_new returned.
    unsafe { sys::g_object_unref(menu) };
    // SAFETY: the popover is new and unparented, and area is the view's live drawing area.
    unsafe { sys::gtk_widget_set_parent(state.ctx_popover, state.area) };
    ns_popover_menu_fit(state.ctx_popover);
    // SAFETY: ctx_popover is the live popover just created.
    unsafe { sys::gtk_popover_set_has_arrow(state.ctx_popover, 0) };
    let rect = sys::GdkRectangle {
        x: state.ctx_x as c_int,
        y: state.ctx_y as c_int,
        width: 1,
        height: 1,
    };
    // SAFETY: ctx_popover is live and rect is valid for the call; GTK copies it.
    unsafe { sys::gtk_popover_set_pointing_to(state.ctx_popover, &rect) };
    // SAFETY: ctx_popover is a live popover parented to the view's widget tree.
    unsafe { sys::gtk_popover_popup(state.ctx_popover) };
}

pub(super) fn request_find(v: *mut NsProcView, direction: c_int) {
    if view(v).opened == 0 {
        return;
    }
    let Some((_, entry, _)) = find_bar(v) else {
        return;
    };
    let query = entry.text();
    let state = view(v);
    state.find_seq += 1;
    let (seq, from_y, case) = (state.find_seq, state.scroll_y, state.find_case);
    push_req(
        v,
        new_req(REQ_FIND, |r| {
            r.seq = seq;
            r.query = glib::strdup(&query);
            r.find_dir = direction;
            r.find_from_y = from_y;
            r.find_case = case;
        }),
    );
}

pub(super) fn search_open(v: *mut NsProcView) {
    let Some((revealer, entry, _)) = find_bar(v) else {
        return;
    };
    revealer.set_reveal_child(true);
    entry.grab_focus();
    entry.select_region(0, -1);
    if !entry.text().is_empty() {
        request_find(v, 0);
    }
}

fn search_close(v: *mut NsProcView) {
    let Some((revealer, _, label)) = find_bar(v) else {
        return;
    };
    revealer.set_reveal_child(false);
    label.set_text(c"");
    let state = view(v);
    state.find_seq += 1;
    let (seq, from_y) = (state.find_seq, state.scroll_y);
    push_req(
        v,
        new_req(REQ_FIND, |r| {
            r.seq = seq;
            r.query = glib::strdup(b"");
            r.find_from_y = from_y;
        }),
    );
    if let Some(area) = area(v) {
        area.grab_focus();
    }
}

pub(super) fn ns_proc_view_find_open(v: *mut NsProcView) {
    search_open(v);
}

pub(super) fn ns_proc_view_find_close(v: *mut NsProcView) -> bool {
    if v.is_null() || !find_bar(v).is_some_and(|(revealer, _, _)| revealer.reveals_child()) {
        return false;
    }
    search_close(v);
    true
}

pub(super) fn ns_proc_view_set_color_scheme(v: *mut NsProcView, dark: bool) {
    if v.is_null() {
        return;
    }
    push_req(v, new_req(REQ_COLOR_SCHEME, |r| r.mods = c_int::from(dark)));
}

fn icon_button(icon: &CStr, tooltip: Option<&CStr>, label: &CStr) -> Button {
    let button = Button::from_icon_name(icon);
    button.set_tooltip_text(tooltip.map(tr));
    button.set_accessible_label(tr(label));
    button
}

pub(super) fn build_search_bar(v: *mut NsProcView, overlay: &Overlay) {
    let bx = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 4);
    bx.add_css_class(c"toolbar");

    let entry = SearchEntry::new();
    entry.set_size_request(220, -1);
    entry.set_accessible_label(tr(c"Find in page"));
    entry.connect_search_changed(move || request_find(v, 0));
    entry.connect_activate(move || request_find(v, 1));
    entry.connect_next_match(move || request_find(v, 1));
    entry.connect_previous_match(move || request_find(v, 2));
    entry.connect_stop_search(move || search_close(v));

    let label = Label::new(c"");
    label.set_size_request(56, -1);

    let prev = icon_button(
        c"go-up-symbolic",
        Some(c"Previous match (Shift+Enter)"),
        c"Previous match",
    );
    let next = icon_button(
        c"go-down-symbolic",
        Some(c"Next match (Enter)"),
        c"Next match",
    );
    let close = icon_button(
        c"window-close-symbolic",
        Some(c"Close (Esc)"),
        c"Close search",
    );
    prev.connect_clicked(move || request_find(v, 2));
    next.connect_clicked(move || request_find(v, 1));
    close.connect_clicked(move || search_close(v));

    bx.append(&entry);
    bx.append(&label);
    bx.append(&prev);
    bx.append(&next);
    bx.append(&close);

    let frame = Frame::new();
    frame.add_css_class(c"ns-findbar");
    frame.set_child(&bx);

    let revealer = Revealer::new();
    revealer.set_transition_type(gtk::REVEALER_TRANSITION_TYPE_SLIDE_DOWN);
    revealer.set_child(&frame);
    revealer.set_halign(sys::ALIGN_END);
    revealer.set_valign(sys::ALIGN_START);
    revealer.set_margins(0, 20, 8, 0);
    overlay.add_overlay(&revealer);

    view(v).ui().find = Some(FindBar {
        revealer,
        entry,
        label,
    });
}

pub(super) fn build_perm_bar(v: *mut NsProcView) -> Revealer {
    let bar = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 8);
    bar.set_margins(8, 8, 4, 4);

    let icon = gtk::Image::from_icon_name(c"dialog-question-symbolic");

    let label = Label::new(c"");
    label.set_xalign(0.0);
    label.set_ellipsize(sys::ELLIPSIZE_MIDDLE);
    label.set_hexpand(true);

    let allow = Button::with_label(tr(c"Allow"));
    allow.add_css_class(c"suggested-action");
    allow.set_tooltip_text(Some(tr(c"Allow and trust this site")));
    let deny = Button::with_label(tr(c"Not now"));
    let close = Button::from_icon_name(c"window-close-symbolic");
    close.set_has_frame(false);
    close.set_accessible_label(tr(c"Dismiss"));
    allow.connect_clicked(move || super::procview_result::perm_resolve(v, true));
    deny.connect_clicked(move || super::procview_result::perm_resolve(v, false));
    close.connect_clicked(move || super::procview_result::perm_resolve(v, false));

    bar.append(&icon);
    bar.append(&label);
    bar.append(&allow);
    bar.append(&deny);
    bar.append(&close);

    let frame = Frame::new();
    frame.set_child(&bar);

    let revealer = Revealer::new();
    revealer.set_transition_type(gtk::REVEALER_TRANSITION_TYPE_SLIDE_DOWN);
    revealer.set_child(&frame);
    view(v).ui().perm = Some(PermBar {
        revealer: revealer.clone(),
        label,
    });
    revealer
}

pub(super) fn ns_pv_console_append(v: *mut NsProcView, text: &[u8]) {
    if text.is_empty() {
        return;
    }
    if let Some(log) = console_text(v, DEV_TAB_CONSOLE) {
        log.append(text);
        log.scroll_to_end();
    }
}

fn console_poll(v: *mut NsProcView) -> ControlFlow<()> {
    let state = view(v);
    if state.console_open != 0 && state.opened != 0 {
        push_req(v, new_req(REQ_CONSOLE, |_| {}));
    }
    ControlFlow::Continue(())
}

fn on_console_eval(v: *mut NsProcView, entry: &Entry) {
    let src = entry.text().into_bytes();
    if src.is_empty() || view(v).opened == 0 {
        return;
    }
    let mut echo = b"> ".to_vec();
    echo.extend_from_slice(&src);
    echo.push(b'\n');
    ns_pv_console_append(v, &echo);
    push_req(v, new_req(REQ_EVAL, |r| r.query = glib::strdup(&src)));
    entry.set_text(c"");
}

pub(super) fn ns_pv_console_set_open(v: *mut NsProcView, open: bool) {
    if open && view(v).ui().console.is_none() {
        build_console_window(v);
    }
    let Some((window, entry)) = console_window(v) else {
        return;
    };
    view(v).console_open = GBoolean::from(open);
    if open {
        if let Some(root) = area(v).and_then(|area| area.root())
            && Window::cast(&root).is_some()
        {
            window.set_transient_for(Some(&root));
        }
        window.present();
        if view(v).ui().console_poll.is_none() {
            let poll = glib::timeout_add_local(CONSOLE_POLL_MS, move || console_poll(v));
            view(v).ui().console_poll = Some(poll);
        }
        entry.grab_focus();
    } else {
        window.set_visible(false);
        if let Some(poll) = view(v).ui().console_poll.take() {
            poll.remove();
        }
        if let Some(area) = area(v) {
            area.grab_focus();
        }
    }
}

fn console_request_dump(v: *mut NsProcView, tab: c_int) {
    let kind: &[u8] = match tab {
        DEV_TAB_NETWORK => b"network",
        DEV_TAB_PERFORMANCE => b"performance",
        DEV_TAB_LAYOUT => b"layout",
        DEV_TAB_ELEMENTS => b"dom",
        _ => return,
    };
    if view(v).opened == 0 {
        return;
    }
    push_req(
        v,
        new_req(REQ_DUMP, |r| {
            r.dump_tab = tab;
            r.query = glib::strdup(kind);
        }),
    );
}

fn console_current_tab(v: *mut NsProcView) -> c_int {
    match view(v).ui().console.as_ref() {
        // SAFETY: the console's notebook is a live GtkNotebook its handle keeps alive.
        Some(console) => unsafe { sys::gtk_notebook_get_current_page(console.notebook.as_ptr()) },
        None => DEV_TAB_CONSOLE,
    }
}

fn on_console_clear(v: *mut NsProcView) {
    if let Some(text) = console_text(v, console_current_tab(v)) {
        text.set_text("");
    }
}

unsafe extern "C" fn on_console_notebook_switch(
    _nb: *mut GtkWidget,
    _page: *mut GtkWidget,
    num: c_uint,
    data: *mut c_void,
) {
    console_request_dump(data.cast(), num as c_int);
}

fn on_inspect_activate(v: *mut NsProcView, entry: &Entry) {
    let selector = entry.text().into_bytes();
    if selector.is_empty() || view(v).opened == 0 {
        return;
    }
    let src = nav::inspect_script(&selector);
    push_req(
        v,
        new_req(REQ_EVAL, |r| {
            r.inspect = 1;
            r.dump_tab = DEV_TAB_ELEMENTS;
            r.query = glib::strdup(&src);
        }),
    );
}

fn console_make_view(wrap: bool) -> (ScrolledWindow, TextView) {
    let text = TextView::new();
    text.set_editable(false);
    text.set_monospace(true);
    text.set_cursor_visible(false);
    text.set_wrap_mode(if wrap {
        sys::WRAP_WORD_CHAR
    } else {
        sys::WRAP_NONE
    });
    let scroll = ScrolledWindow::new();
    scroll.set_child(&text);
    scroll.set_vexpand(true);
    (scroll, text)
}

fn console_add_tab(notebook: &Widget, child: &impl AsRef<Widget>, title: &CStr) {
    let label = Label::new(tr(title));
    // SAFETY: notebook is a live GtkNotebook and both widgets are live; the notebook takes its own references.
    unsafe {
        sys::gtk_notebook_append_page(notebook.as_ptr(), child.as_ref().as_ptr(), label.as_ptr())
    };
}

fn new_notebook() -> Widget {
    // SAFETY: gtk_notebook_new has no preconditions and returns a new floating widget.
    let notebook = unsafe { sys::gtk_notebook_new() };
    // SAFETY: notebook is the live widget just created; the handle sinks its floating reference.
    unsafe { Widget::from_ptr(notebook) }.expect("gtk_notebook_new never returns null")
}

fn build_console_window(v: *mut NsProcView) {
    let bx = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 0);

    let header = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 4);
    header.add_css_class(c"toolbar");
    let spacer = Label::new(c"");
    spacer.set_hexpand(true);
    let refresh = icon_button(c"view-refresh-symbolic", Some(c"Refresh"), c"Refresh");
    refresh.connect_clicked(move || console_request_dump(v, console_current_tab(v)));
    let clear = icon_button(c"edit-clear-symbolic", Some(c"Clear"), c"Clear");
    clear.connect_clicked(move || on_console_clear(v));
    header.append(&spacer);
    header.append(&refresh);
    header.append(&clear);

    let notebook = new_notebook();
    notebook.set_vexpand(true);

    let console_page = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 0);
    let (console_scroll, log) = console_make_view(true);
    let entry = Entry::new();
    entry.set_placeholder_text(tr(c"Evaluate JavaScript and press Enter"));
    let weak = entry.downgrade();
    entry.connect_activate(move || {
        if let Some(entry) = weak.upgrade().as_ref().and_then(Entry::cast) {
            on_console_eval(v, &entry);
        }
    });
    console_page.append(&console_scroll);
    console_page.append(&entry);
    console_add_tab(&notebook, &console_page, c"Console");

    let (net_scroll, network) = console_make_view(false);
    console_add_tab(&notebook, &net_scroll, c"Network");
    let (perf_scroll, performance) = console_make_view(false);
    console_add_tab(&notebook, &perf_scroll, c"Performance");
    let (layout_scroll, layout) = console_make_view(false);
    console_add_tab(&notebook, &layout_scroll, c"Layout");

    let elements_page = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 0);
    let (elements_scroll, elements) = console_make_view(false);
    let inspect = Entry::new();
    inspect.set_placeholder_text(tr(c"Inspect: CSS selector, then Enter"));
    let weak = inspect.downgrade();
    inspect.connect_activate(move || {
        if let Some(entry) = weak.upgrade().as_ref().and_then(Entry::cast) {
            on_inspect_activate(v, &entry);
        }
    });
    elements_page.append(&elements_scroll);
    elements_page.append(&inspect);
    console_add_tab(&notebook, &elements_page, c"Elements");

    let on_switch = handler!(on_console_notebook_switch);
    // SAFETY: the notebook is live and switch-page passes (notebook, page, number, data), which the handler takes.
    unsafe { connect(notebook.as_ptr(), c"switch-page", on_switch, v.cast()) };

    bx.append(&header);
    bx.append(&notebook);

    let window = Window::new();
    window.set_title(tr(c"Developer Tools"));
    window.set_default_size(720, 420);
    window.set_destroy_with_parent(true);
    window.set_child(&bx);
    window.connect_close_request(move || {
        ns_pv_console_set_open(v, false);
        true
    });

    view(v).ui().console = Some(Console {
        window,
        notebook,
        entry,
        log,
        network,
        performance,
        layout,
        elements,
    });
}

pub(super) fn ns_proc_view_print(v: *mut NsProcView) {
    if v.is_null() || view(v).opened == 0 {
        return;
    }
    push_req(v, new_req(REQ_PRINT, |_| {}));
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
