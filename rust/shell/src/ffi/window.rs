//! Southstar — the tabbed browser window: tab strip, toolbar, address bar, status pill, actions, menu and bookmarks popover.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_ulong, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_bookmarks::Bookmarks;
use southstar_config::NsConfig;
use southstar_gtk_ffi::{self as gtk, GBoolean, GdkRgba, GtkWidget};

use super::appmain::ns_app_self_exe;
use super::gtk::{
    borrowed, connect, cstring, handler, set_accessible_label, set_margins, take_gstring,
    toggle_css_class, tr, tr_string,
};
use super::icons::{
    ns_icon_button_new, ns_icon_button_set, ns_icon_entry_set, ns_icon_image_new,
    ns_icon_image_set, ns_icon_menu_button_set,
};
use super::procview_chrome::ns_popover_menu_fit;
use super::titlerow::ns_title_row_new;
use super::view::{self, View};
use super::{downloads, taskmgr};
use crate::window as rules;

unsafe extern "C" {
    fn ns_url_from_local_path(path: *const c_char) -> *mut c_char;
    fn ns_address_is_search(s: *const c_char) -> GBoolean;
    fn ns_search_url_for(query: *const c_char) -> *mut c_char;
    fn ns_browser_set_color_scheme(dark: c_int);
}

const WINDOW_KEY: &CStr = c"ns-procwindow";
const PW_KEY: &CStr = c"ns-pw";
const PAGE_KEY: &CStr = c"ns-page";
const TAB_CLOSE_KEY: &CStr = c"ns-tab-close";
const TAB_LABEL_KEY: &CStr = c"ns-tab-label";
const TAB_ICON_KEY: &CStr = c"ns-tab-icon";
const TAB_SPINNER_KEY: &CStr = c"ns-tab-spinner";
const STRIP_TAB_KEY: &CStr = c"ns-strip-tab";
const BOOKMARK_URL_KEY: &CStr = c"ns-bm-url";

const EVT_TITLE: c_int = 0;
const EVT_URL: c_int = 1;
const EVT_STATUS: c_int = 2;
const EVT_NEWTAB: c_int = 3;
const EVT_HISTORY: c_int = 4;
const EVT_LOADING: c_int = 5;
const EVT_DOWNLOAD: c_int = 6;
const EVT_FAVICON: c_int = 7;
const EVT_WEBGL: c_int = 8;
const EVT_FULLSCREEN: c_int = 9;
const EVT_ZOOM: c_int = 10;

const COLOR_SCHEME_LIGHT: c_int = 1;
const COLOR_SCHEME_DARK: c_int = 2;

pub struct ProcWindow {
    app: *mut c_void,
    pub window: Cell<*mut GtkWidget>,
    header: Cell<*mut GtkWidget>,
    toolbar: Cell<*mut GtkWidget>,
    notebook: Cell<*mut GtkWidget>,
    tabstrip: Cell<*mut GtkWidget>,
    newtab_btn: Cell<*mut GtkWidget>,
    address: Cell<*mut GtkWidget>,
    address_click_focuses: Cell<bool>,
    zoom_button: Cell<*mut GtkWidget>,
    back: Cell<*mut GtkWidget>,
    forward: Cell<*mut GtkWidget>,
    reload: Cell<*mut GtkWidget>,
    loading: Cell<bool>,
    status: Cell<*mut GtkWidget>,
    status_base: RefCell<CString>,
    status_timer: Cell<c_uint>,
    theme_watch: Cell<[c_ulong; 2]>,
    webgl_active: Cell<bool>,
    element_fullscreen: Cell<bool>,
    fullscreen_view: Cell<Option<View>>,
    fullscreen_notice: Cell<*mut GtkWidget>,
    fullscreen_notice_timer: Cell<c_uint>,
    bookmarks: RefCell<Bookmarks>,
    pub session_path: RefCell<Option<CString>>,
    pub session_timer: Cell<c_uint>,
}

type Pw = *const ProcWindow;

unsafe fn pw<'a>(data: *mut c_void) -> &'a ProcWindow {
    unsafe { &*data.cast::<ProcWindow>() }
}

fn data(pw: &ProcWindow) -> *mut c_void {
    (pw as Pw).cast_mut().cast()
}

fn brand_versioned() -> &'static CStr {
    static BRAND: OnceLock<CString> = OnceLock::new();
    BRAND.get_or_init(|| {
        cstring(format!(
            "{} {}",
            tr_string(c"Southstar Browser"),
            crate::titlerow::VERSION
        ))
    })
}

fn config() -> Option<&'static NsConfig> {
    southstar_config::get()
}

unsafe extern "C" fn procwindow_free(data: *mut c_void) {
    unsafe {
        let pw = Box::from_raw(data.cast::<ProcWindow>());
        for timer in [
            pw.session_timer.get(),
            pw.status_timer.get(),
            pw.fullscreen_notice_timer.get(),
        ] {
            if timer != 0 {
                gtk::g_source_remove(timer);
            }
        }
        let settings = gtk::gtk_settings_get_default();
        if !settings.is_null() {
            for id in pw.theme_watch.get() {
                if id != 0 {
                    gtk::g_signal_handler_disconnect(settings, id);
                }
            }
        }
    }
}

pub unsafe fn install_icon_search_paths() {
    unsafe {
        let display = gtk::gdk_display_get_default();
        if display.is_null() {
            return;
        }
        let theme = gtk::gtk_icon_theme_get_for_display(display);
        if theme.is_null() {
            return;
        }
        gtk::gtk_icon_theme_add_resource_path(theme, c"/org/southstar/WebBrowser/icons".as_ptr());
        let Some(exe) = borrowed(ns_app_self_exe()) else {
            return;
        };
        let Some(dir) = take_gstring(gtk::g_path_get_dirname(exe.as_ptr())) else {
            return;
        };
        for rel in rules::ICON_SEARCH_PATHS {
            let p = super::gtk::build_filename(&[&dir, rel]);
            gtk::gtk_icon_theme_add_search_path(theme, p.as_ptr());
        }
    }
}

pub unsafe fn install_chrome_css() {
    unsafe {
        let display = gtk::gdk_display_get_default();
        if display.is_null() {
            return;
        }
        let p = gtk::gtk_css_provider_new();
        let css = cstring(crate::chrome::CHROME_CSS);
        gtk::gtk_css_provider_load_from_string(p, css.as_ptr());
        gtk::gtk_style_context_add_provider_for_display(
            display,
            p,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        gtk::g_object_unref(p);
    }
}

unsafe fn toolbar_button(
    icon: &CStr,
    tooltip: &CStr,
    cb: gtk::GCallback,
    pw: &ProcWindow,
) -> *mut GtkWidget {
    unsafe {
        let b = ns_icon_button_new(icon);
        gtk::gtk_button_set_has_frame(b, 0);
        gtk::gtk_widget_add_css_class(b, c"ns-nav-button".as_ptr());
        gtk::gtk_widget_set_valign(b, gtk::ALIGN_CENTER);
        gtk::gtk_widget_set_tooltip_text(b, tooltip.as_ptr());
        set_accessible_label(b, tooltip);
        connect(b, c"clicked", cb, data(pw));
        b
    }
}

impl ProcWindow {
    fn notebook(&self) -> *mut GtkWidget {
        self.notebook.get()
    }

    fn current_view(&self) -> Option<View> {
        unsafe {
            let idx = gtk::gtk_notebook_get_current_page(self.notebook());
            if idx < 0 {
                return None;
            }
            view::for_page(gtk::gtk_notebook_get_nth_page(self.notebook(), idx))
        }
    }

    fn set_loading_ui(&self, loading: bool) {
        self.loading.set(loading);
        let tip = if loading {
            tr(c"Stop loading this page")
        } else {
            tr(c"Reload this page")
        };
        let icon = if loading {
            c"southstar-stop-symbolic"
        } else {
            c"southstar-reload-symbolic"
        };
        unsafe {
            ns_icon_button_set(self.reload.get().cast(), icon);
            gtk::gtk_widget_set_tooltip_text(self.reload.get(), tip.as_ptr());
            set_accessible_label(self.reload.get(), tip);
        }
    }

    fn set_address_text(&self, url: Option<&CStr>) {
        let shown = address_display_url(url);
        unsafe { gtk::gtk_editable_set_text(self.address.get(), shown.as_ptr()) };
    }

    fn update_security_indicator(&self, v: Option<View>) {
        let entry = self.address.get();
        if entry.is_null() {
            return;
        }
        let url = v.and_then(View::url);
        let sec = v.map_or(0, View::security);
        let (icon_name, label) = match sec {
            rules::SECURITY_SECURE => (
                Some(c"southstar-lock-symbolic"),
                Some(tr(c"Secure — the certificate is valid")),
            ),
            rules::SECURITY_INVALID => (
                Some(c"southstar-warning-symbolic"),
                Some(tr(c"Not secure — the certificate is not trusted")),
            ),
            rules::SECURITY_PLAIN => (
                Some(c"southstar-lock-open-symbolic"),
                Some(tr(c"Not secure — the connection is not encrypted")),
            ),
            _ => (None, None),
        };
        let has_url = url.as_deref().is_some_and(|u| !u.is_empty());
        unsafe {
            toggle_css_class(
                entry,
                c"ns-insecure",
                has_url && sec == rules::SECURITY_INVALID,
            );
            let (Some(icon_name), Some(label), true) = (icon_name, label, has_url) else {
                let internal = url
                    .as_deref()
                    .is_some_and(|u| u.to_bytes().starts_with(b"about:"));
                let page_icon = if internal {
                    c"southstar"
                } else {
                    c"southstar-globe-symbolic"
                };
                ns_icon_entry_set(entry.cast(), gtk::ENTRY_ICON_PRIMARY, page_icon);
                gtk::gtk_entry_set_icon_activatable(entry, gtk::ENTRY_ICON_PRIMARY, 0);
                gtk::gtk_entry_set_icon_tooltip_text(
                    entry,
                    gtk::ENTRY_ICON_PRIMARY,
                    tr(c"Page location").as_ptr(),
                );
                return;
            };
            ns_icon_entry_set(entry.cast(), gtk::ENTRY_ICON_PRIMARY, icon_name);
            gtk::gtk_entry_set_icon_activatable(entry, gtk::ENTRY_ICON_PRIMARY, 0);
            let mut tip = label.to_bytes().to_vec();
            let url = url.unwrap_or_default();
            if let Some(host) = southstar_url::bytes::host_from(url.to_bytes())
                && !host.is_empty()
            {
                tip.push(b'\n');
                tip.extend_from_slice(&host);
            }
            if let Some(ip) = v.and_then(View::remote_ip).filter(|ip| !ip.is_empty()) {
                tip.push(b'\n');
                tip.extend_from_slice(tr(c"Server:").to_bytes());
                tip.push(b' ');
                tip.extend_from_slice(ip.to_bytes());
            }
            let tip = cstring(tip);
            gtk::gtk_entry_set_icon_tooltip_text(entry, gtk::ENTRY_ICON_PRIMARY, tip.as_ptr());
        }
    }

    fn apply_color_scheme(&self) {
        let dark = match config().map_or(0, |c| c.color_scheme) {
            COLOR_SCHEME_LIGHT => false,
            COLOR_SCHEME_DARK => true,
            _ => {
                let mut fg = GdkRgba::default();
                unsafe { gtk::gtk_widget_get_color(self.window.get(), &mut fg) };
                rules::prefers_dark(fg.red, fg.green, fg.blue)
            }
        };
        let want = if dark { c"dark" } else { c"light" };
        unsafe {
            if borrowed(gtk::g_getenv(c"NS_COLOR_SCHEME".as_ptr())) == Some(want) {
                return;
            }
            gtk::g_setenv(c"NS_COLOR_SCHEME".as_ptr(), want.as_ptr(), 1);
            ns_browser_set_color_scheme(c_int::from(dark));
            for v in view::views(self.notebook()) {
                v.set_color_scheme(dark);
                if v.url().is_some_and(|u| !u.is_empty()) {
                    v.reload();
                }
            }
        }
    }

    fn update_zoom_indicator(&self, v: Option<View>) {
        let button = self.zoom_button.get();
        if button.is_null() {
            return;
        }
        let percent = v.map_or(100, View::zoom_percent);
        unsafe {
            if percent == 100 {
                gtk::gtk_widget_set_visible(button, 0);
                return;
            }
            let label = cstring(format!("{percent}%"));
            gtk::gtk_button_set_label(button, label.as_ptr());
            gtk::gtk_widget_set_visible(button, 1);
        }
    }

    fn current_page_bookmarked(&self) -> bool {
        let url = self.current_view().and_then(View::url);
        url.is_some_and(|url| !url.is_empty() && self.bookmarks.borrow().contains(&url))
    }

    fn update_bookmark_indicator(&self) {
        let entry = self.address.get();
        let url = self.current_view().and_then(View::url);
        let saved = self.current_page_bookmarked();
        unsafe {
            toggle_css_class(entry, c"ns-bookmarked", saved);
            if !url.is_some_and(|u| !u.is_empty()) {
                southstar_gtk_ffi::gtk_entry_set_icon_from_icon_name(
                    entry.cast(),
                    gtk::ENTRY_ICON_SECONDARY,
                    ptr::null(),
                );
                return;
            }
            let star = if saved {
                c"southstar-star-filled-symbolic"
            } else {
                c"southstar-star-symbolic"
            };
            ns_icon_entry_set(entry.cast(), gtk::ENTRY_ICON_SECONDARY, star);
            let tip = if saved {
                tr(c"Remove this bookmark")
            } else {
                tr(c"Bookmark this page")
            };
            gtk::gtk_entry_set_icon_tooltip_text(entry, gtk::ENTRY_ICON_SECONDARY, tip.as_ptr());
        }
    }

    fn update_chrome(&self) {
        let window = self.window.get();
        unsafe {
            let Some(v) = self.current_view() else {
                gtk::gtk_editable_set_text(self.address.get(), c"".as_ptr());
                gtk::gtk_window_set_title(window, brand_versioned().as_ptr());
                gtk::gtk_widget_set_sensitive(self.back.get(), 0);
                gtk::gtk_widget_set_sensitive(self.forward.get(), 0);
                self.update_security_indicator(None);
                self.set_loading_ui(false);
                return;
            };
            self.set_loading_ui(v.is_loading());
            toggle_css_class(self.toolbar.get(), c"ns-private", v.is_private());
            let url = v.url();
            let title = v.title();
            self.set_address_text(url.as_deref());
            let wt = cstring(rules::window_title(
                title.as_deref().map(CStr::to_bytes),
                brand_versioned().to_bytes(),
            ));
            gtk::gtk_window_set_title(window, wt.as_ptr());
            gtk::gtk_widget_set_sensitive(self.back.get(), GBoolean::from(v.can_back()));
            gtk::gtk_widget_set_sensitive(self.forward.get(), GBoolean::from(v.can_forward()));
            self.update_security_indicator(Some(v));
            self.update_zoom_indicator(Some(v));
            self.update_bookmark_indicator();
        }
    }

    fn render_status(&self) {
        let base = self.status_base.borrow().clone();
        let webgl = self.webgl_active.get();
        let note = tr(c"WebGL enabled");
        let text = cstring(rules::status_text(
            base.to_bytes(),
            webgl.then_some(note.to_bytes()),
        ));
        unsafe {
            gtk::gtk_label_set_text(self.status.get(), text.as_ptr());
            gtk::gtk_widget_set_visible(
                self.status.get(),
                GBoolean::from(!self.element_fullscreen.get() && (!base.is_empty() || webgl)),
            );
        }
    }

    fn set_status_persistent(&self, text: Option<&CStr>) {
        let timer = self.status_timer.replace(0);
        if timer != 0 {
            unsafe { gtk::g_source_remove(timer) };
        }
        *self.status_base.borrow_mut() = text.unwrap_or(c"").to_owned();
        self.render_status();
    }

    fn set_status(&self, text: &CStr) {
        let timer = self.status_timer.get();
        if timer != 0 {
            unsafe { gtk::g_source_remove(timer) };
        }
        *self.status_base.borrow_mut() = text.to_owned();
        self.render_status();
        let timer = if text.is_empty() {
            0
        } else {
            unsafe { gtk::g_timeout_add_seconds(5, status_expire, data(self)) }
        };
        self.status_timer.set(timer);
    }

    fn hide_fullscreen_notice(&self) {
        let timer = self.fullscreen_notice_timer.replace(0);
        unsafe {
            if timer != 0 {
                gtk::g_source_remove(timer);
            }
            gtk::gtk_widget_set_visible(self.fullscreen_notice.get(), 0);
        }
    }

    fn show_fullscreen_notice(&self, v: Option<View>) {
        let url = v.and_then(View::url);
        let host = url
            .and_then(|u| southstar_url::bytes::host_from(u.to_bytes()))
            .and_then(|host| CString::new(host).ok());
        let site = host
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| tr(c"This page").to_owned());
        unsafe {
            let site_markup =
                take_gstring(gtk::g_markup_escape_text(site.as_ptr(), -1)).unwrap_or_default();
            let hint = take_gstring(gtk::g_markup_escape_text(
                tr(c"is now full screen — press Esc to exit").as_ptr(),
                -1,
            ))
            .unwrap_or_default();
            let markup = cstring(
                [
                    b"<b>".as_slice(),
                    site_markup.to_bytes(),
                    b"</b> ",
                    hint.to_bytes(),
                ]
                .concat(),
            );
            let notice = self.fullscreen_notice.get();
            gtk::gtk_label_set_markup(notice, markup.as_ptr());
            gtk::gtk_widget_set_visible(notice, 1);
            let timer = self.fullscreen_notice_timer.get();
            if timer != 0 {
                gtk::g_source_remove(timer);
            }
            self.fullscreen_notice_timer.set(gtk::g_timeout_add_seconds(
                rules::FULLSCREEN_NOTICE_SECONDS,
                fullscreen_notice_expire,
                data(self),
            ));
        }
    }

    fn set_element_fullscreen(&self, active: bool) {
        if self.element_fullscreen.get() == active {
            return;
        }
        self.element_fullscreen.set(active);
        self.fullscreen_view
            .set(if active { self.current_view() } else { None });
        unsafe {
            gtk::gtk_widget_set_visible(self.header.get(), GBoolean::from(!active));
            gtk::gtk_widget_set_visible(self.toolbar.get(), GBoolean::from(!active));
            self.render_status();
            if active {
                gtk::gtk_window_fullscreen(self.window.get());
                self.show_fullscreen_notice(self.fullscreen_view.get());
            } else {
                self.hide_fullscreen_notice();
                gtk::gtk_window_unfullscreen(self.window.get());
            }
        }
    }

    fn leave_element_fullscreen(&self) {
        if !self.element_fullscreen.get() {
            return;
        }
        let v = self.fullscreen_view.get();
        self.set_element_fullscreen(false);
        if let Some(v) = v {
            v.exit_fullscreen();
        }
    }

    fn page_index(&self, page: *mut GtkWidget) -> c_int {
        if page.is_null() {
            -1
        } else {
            unsafe { gtk::gtk_notebook_page_num(self.notebook(), page) }
        }
    }

    fn nth_page(&self, idx: c_int) -> *mut GtkWidget {
        unsafe { gtk::gtk_notebook_get_nth_page(self.notebook(), idx) }
    }

    fn update_active_tab(&self) {
        unsafe {
            let idx = gtk::gtk_notebook_get_current_page(self.notebook());
            let current = if idx >= 0 {
                self.nth_page(idx)
            } else {
                ptr::null_mut()
            };
            let closable = gtk::gtk_notebook_get_n_pages(self.notebook()) > 1;
            let mut w = gtk::gtk_widget_get_first_child(self.tabstrip.get());
            while !w.is_null() {
                let page = gtk::g_object_get_data(w.cast(), PAGE_KEY.as_ptr()).cast::<GtkWidget>();
                if !page.is_null() {
                    toggle_css_class(w, c"ns-tab-active", page == current);
                    let close = gtk::g_object_get_data(w.cast(), TAB_CLOSE_KEY.as_ptr())
                        .cast::<GtkWidget>();
                    if !close.is_null() {
                        gtk::gtk_widget_set_visible(close, GBoolean::from(closable));
                    }
                }
                w = gtk::gtk_widget_get_next_sibling(w);
            }
        }
    }

    fn close_page(&self, page: *mut GtkWidget) {
        unsafe {
            let wrapper =
                gtk::g_object_get_data(page.cast(), STRIP_TAB_KEY.as_ptr()).cast::<GtkWidget>();
            let idx = self.page_index(page);
            if self.element_fullscreen.get() && view::for_page(page) == self.fullscreen_view.get() {
                self.set_element_fullscreen(false);
            }
            if idx >= 0 {
                gtk::gtk_notebook_remove_page(self.notebook(), idx);
            }
            if !wrapper.is_null() {
                gtk::gtk_box_remove(self.tabstrip.get(), wrapper);
            }
            if gtk::gtk_notebook_get_n_pages(self.notebook()) == 0 {
                gtk::gtk_window_close(self.window.get());
            } else {
                self.update_active_tab();
            }
        }
    }

    pub fn add_tab_full(&self, url: &CStr, foreground: bool, private_mode: bool) {
        unsafe {
            if !private_mode
                && southstar_renderer_host::single_process_enabled()
                && gtk::gtk_notebook_get_n_pages(self.notebook()) > 0
                && let Some(cur) = self.current_view()
            {
                let r = normalize_url(url);
                gtk::gtk_editable_set_text(self.address.get(), r.as_ptr());
                cur.load(&r);
                return;
            }

            let Some(v) = View::create() else {
                return;
            };
            if private_mode {
                v.set_private(true);
            }
            v.set_notify(on_view_notify, data(self));
            let page = v.widget();
            gtk::g_object_set_data(page.cast(), view::PAGE_VIEW_KEY.as_ptr(), v.raw().cast());

            let wrapper = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 0);
            gtk::gtk_widget_add_css_class(wrapper, c"ns-tab".as_ptr());
            if private_mode {
                gtk::gtk_widget_add_css_class(wrapper, c"ns-tab-private".as_ptr());
            }

            let tabbtn = gtk::gtk_button_new();
            gtk::gtk_button_set_has_frame(tabbtn, 0);
            gtk::gtk_widget_add_css_class(tabbtn, c"ns-tab-label".as_ptr());
            let tabcontent = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 5);
            let icon = ns_icon_image_new(if private_mode {
                c"southstar-private-symbolic"
            } else {
                c"southstar-page-symbolic"
            });
            gtk::gtk_image_set_pixel_size(icon, 16);
            let spinner = gtk::gtk_spinner_new();
            gtk::gtk_widget_set_size_request(spinner, 16, 16);
            gtk::gtk_widget_set_visible(spinner, 0);
            if private_mode {
                gtk::gtk_widget_set_tooltip_text(tabbtn, tr(c"Private tab").as_ptr());
            }
            let label = gtk::gtk_label_new(
                if private_mode {
                    tr(c"Private tab")
                } else {
                    tr(c"New Tab")
                }
                .as_ptr(),
            );
            gtk::gtk_label_set_ellipsize(label, gtk::ELLIPSIZE_END);
            gtk::gtk_label_set_width_chars(label, 10);
            gtk::gtk_label_set_max_width_chars(label, 22);
            gtk::gtk_label_set_xalign(label, 0.0);
            gtk::gtk_box_append(tabcontent, spinner);
            gtk::gtk_box_append(tabcontent, icon);
            gtk::gtk_box_append(tabcontent, label);
            southstar_gtk_ffi::gtk_button_set_child(tabbtn.cast(), tabcontent);
            gtk::g_object_set_data(tabbtn.cast(), PW_KEY.as_ptr(), data(self));
            connect(tabbtn, c"clicked", handler!(on_tab_clicked), page.cast());
            gtk::gtk_box_append(wrapper, tabbtn);

            let close = ns_icon_button_new(c"southstar-close-symbolic");
            gtk::gtk_button_set_has_frame(close, 0);
            gtk::gtk_widget_add_css_class(close, c"ns-tab-close".as_ptr());
            gtk::gtk_widget_set_valign(close, gtk::ALIGN_CENTER);
            gtk::gtk_widget_set_tooltip_text(close, tr(c"Close tab").as_ptr());
            set_accessible_label(close, tr(c"Close tab"));
            gtk::g_object_set_data(close.cast(), PW_KEY.as_ptr(), data(self));
            connect(close, c"clicked", handler!(on_tab_close), page.cast());
            gtk::gtk_box_append(wrapper, close);

            gtk::g_object_set_data(wrapper.cast(), PAGE_KEY.as_ptr(), page.cast());
            gtk::g_object_set_data(wrapper.cast(), TAB_CLOSE_KEY.as_ptr(), close.cast());
            gtk::g_object_set_data(page.cast(), TAB_LABEL_KEY.as_ptr(), label.cast());
            gtk::g_object_set_data(page.cast(), TAB_ICON_KEY.as_ptr(), icon.cast());
            gtk::g_object_set_data(page.cast(), TAB_SPINNER_KEY.as_ptr(), spinner.cast());
            gtk::g_object_set_data(page.cast(), STRIP_TAB_KEY.as_ptr(), wrapper.cast());

            let blank = gtk::gtk_label_new(ptr::null());
            let idx = gtk::gtk_notebook_append_page(self.notebook(), page, blank);

            let newtab = self.newtab_btn.get();
            gtk::g_object_ref(newtab.cast());
            gtk::gtk_box_remove(self.tabstrip.get(), newtab);
            gtk::gtk_box_append(self.tabstrip.get(), wrapper);
            gtk::gtk_box_append(self.tabstrip.get(), newtab);
            gtk::g_object_unref(newtab.cast());

            if foreground {
                gtk::gtk_notebook_set_current_page(self.notebook(), idx);
            }
            self.update_active_tab();

            let resolved = normalize_url(url);
            v.load(&resolved);
        }
    }

    pub fn add_tab(&self, url: &CStr, foreground: bool) {
        self.add_tab_full(url, foreground, false);
    }

    fn address_text(&self) -> CString {
        unsafe { borrowed(gtk::gtk_editable_get_text(self.address.get())) }
            .unwrap_or(c"")
            .to_owned()
    }

    fn activate_address(&self) {
        let resolved = normalize_url(&self.address_text());
        if resolved.is_empty() {
            return;
        }
        let v = match self.current_view() {
            None => {
                self.add_tab(&resolved, true);
                self.current_view()
            }
            Some(v) => {
                unsafe { gtk::gtk_editable_set_text(self.address.get(), resolved.as_ptr()) };
                v.load(&resolved);
                Some(v)
            }
        };
        if let Some(v) = v {
            v.focus();
        }
    }

    fn toggle_bookmark(&self) -> bool {
        let Some(v) = self.current_view() else {
            return false;
        };
        let Some(url) = v.url().filter(|u| !u.is_empty()) else {
            return false;
        };
        let saved = self.bookmarks.borrow().contains(&url);
        if saved {
            self.bookmarks.borrow_mut().remove(&url);
            self.set_status(tr(c"Bookmark removed"));
        } else {
            let title = v.title();
            self.bookmarks.borrow_mut().add(&url, title.as_deref());
            self.set_status(tr(c"Bookmark added"));
        }
        self.update_bookmark_indicator();
        true
    }

    fn toggle_window_fullscreen(&self) {
        let win = self.window.get();
        unsafe {
            if gtk::gtk_window_is_fullscreen(win) != 0 {
                gtk::gtk_window_unfullscreen(win);
                gtk::gtk_window_maximize(win);
            } else {
                gtk::gtk_window_fullscreen(win);
            }
        }
    }

    fn load_in_current(&self, url: &CStr) {
        if let Some(v) = self.current_view() {
            v.load(url);
        }
    }
}

pub fn normalize_url(input: &CStr) -> CString {
    let trimmed = rules::strip(input.to_bytes());
    if trimmed.is_empty() || rules::is_explicit_url(trimmed) {
        return cstring(trimmed);
    }
    let trimmed = cstring(trimmed);
    unsafe {
        if let Some(local) = take_gstring(ns_url_from_local_path(trimmed.as_ptr())) {
            return local;
        }
        if ns_address_is_search(trimmed.as_ptr()) != 0 {
            return take_gstring(ns_search_url_for(trimmed.as_ptr())).unwrap_or_default();
        }
    }
    cstring(rules::https_url(trimmed.to_bytes()))
}

fn address_display_url(url: Option<&CStr>) -> CString {
    let Some(url) = url.filter(|u| !u.is_empty()) else {
        return CString::default();
    };
    if !url.to_bytes().contains(&b'%') {
        return url.to_owned();
    }
    let Some(dec) =
        (unsafe { take_gstring(gtk::g_uri_unescape_string(url.as_ptr(), ptr::null())) })
    else {
        return url.to_owned();
    };
    match dec.to_str() {
        Ok(text) if rules::display_is_safe(text) => dec,
        _ => url.to_owned(),
    }
}

unsafe extern "C" fn status_expire(data: *mut c_void) -> GBoolean {
    let pw = unsafe { pw(data) };
    pw.status_timer.set(0);
    *pw.status_base.borrow_mut() = CString::default();
    pw.render_status();
    gtk::SOURCE_REMOVE
}

unsafe extern "C" fn fullscreen_notice_expire(data: *mut c_void) -> GBoolean {
    let pw = unsafe { pw(data) };
    pw.fullscreen_notice_timer.set(0);
    unsafe { gtk::gtk_widget_set_visible(pw.fullscreen_notice.get(), 0) };
    gtk::SOURCE_REMOVE
}

unsafe fn set_tab_loading(page: *mut GtkWidget, loading: bool) {
    unsafe {
        let icon = gtk::g_object_get_data(page.cast(), TAB_ICON_KEY.as_ptr()).cast::<GtkWidget>();
        let spinner =
            gtk::g_object_get_data(page.cast(), TAB_SPINNER_KEY.as_ptr()).cast::<GtkWidget>();
        if icon.is_null() || spinner.is_null() {
            return;
        }
        gtk::gtk_spinner_set_spinning(spinner, GBoolean::from(loading));
        gtk::gtk_widget_set_visible(spinner, GBoolean::from(loading));
        gtk::gtk_widget_set_visible(icon, GBoolean::from(!loading));
    }
}

unsafe extern "C" fn on_view_notify(
    raw: *mut view::NsProcView,
    evt: c_int,
    text: *const c_char,
    data: *mut c_void,
) {
    let pw = unsafe { pw(data) };
    let Some(v) = (unsafe { View::from_raw(raw.cast()) }) else {
        return;
    };
    let text = unsafe { borrowed(text) };
    let page = v.widget();
    let idx = pw.page_index(page);
    let is_current = Some(v) == pw.current_view();
    unsafe {
        match evt {
            EVT_TITLE => {
                if !v.is_private() {
                    let url = v.url();
                    southstar_history::record(url.as_deref(), text);
                }
                if idx >= 0 {
                    let p = pw.nth_page(idx);
                    let label = gtk::g_object_get_data(p.cast(), TAB_LABEL_KEY.as_ptr())
                        .cast::<GtkWidget>();
                    let t = text.filter(|t| !t.is_empty()).unwrap_or(tr(c"Untitled"));
                    let clip = cstring(rules::clip_title(t.to_bytes()));
                    if !label.is_null() {
                        gtk::gtk_label_set_text(label, clip.as_ptr());
                    }
                }
                if is_current {
                    pw.update_chrome();
                }
            }
            EVT_URL => {
                if pw.element_fullscreen.get() && Some(v) == pw.fullscreen_view.get() {
                    pw.set_element_fullscreen(false);
                }
                if is_current {
                    pw.set_address_text(text);
                    pw.webgl_active.set(false);
                    pw.set_status_persistent(None);
                }
            }
            EVT_STATUS => {
                if is_current {
                    pw.set_status_persistent(text);
                }
            }
            EVT_WEBGL => {
                if is_current && !pw.webgl_active.get() {
                    pw.webgl_active.set(true);
                    pw.render_status();
                }
            }
            EVT_FULLSCREEN => {
                if is_current {
                    pw.set_element_fullscreen(
                        text.is_some_and(|t| t.to_bytes() == b"fullscreen-enter"),
                    );
                }
            }
            EVT_HISTORY => {
                if is_current {
                    gtk::gtk_widget_set_sensitive(pw.back.get(), GBoolean::from(v.can_back()));
                    gtk::gtk_widget_set_sensitive(
                        pw.forward.get(),
                        GBoolean::from(v.can_forward()),
                    );
                }
            }
            EVT_NEWTAB => {
                if let Some(t) = text.filter(|t| !t.is_empty()) {
                    pw.add_tab_full(t, false, v.is_private());
                }
            }
            EVT_LOADING => {
                let loading = text.is_some_and(|t| t.to_bytes().first() == Some(&b'1'));
                if idx >= 0 {
                    set_tab_loading(pw.nth_page(idx), loading);
                }
                if is_current {
                    let was_loading = pw.loading.get();
                    pw.set_loading_ui(loading);
                    if was_loading && !loading {
                        pw.set_status(tr(c"Done"));
                    }
                }
            }
            EVT_ZOOM => {
                if is_current {
                    pw.update_zoom_indicator(Some(v));
                }
            }
            EVT_DOWNLOAD => {
                if let Some(t) = text.filter(|t| !t.is_empty()) {
                    let (target, name) = rules::split_download(t.to_bytes());
                    let page_url = v.url();
                    let local_target = rules::has_file_scheme(target);
                    let local_page = page_url
                        .as_deref()
                        .is_some_and(|u| rules::has_file_scheme(u.to_bytes()));
                    if !local_target || local_page {
                        let target = cstring(target);
                        let name = name.filter(|n| !n.is_empty()).map(cstring);
                        downloads::start(pw.window.get(), &target, name.as_deref());
                    }
                }
            }
            EVT_FAVICON if idx >= 0 && !v.is_private() => {
                let p = pw.nth_page(idx);
                let icon =
                    gtk::g_object_get_data(p.cast(), TAB_ICON_KEY.as_ptr()).cast::<GtkWidget>();
                if !icon.is_null() {
                    let fav = v.favicon();
                    if fav.is_null() {
                        ns_icon_image_set(icon.cast(), c"southstar-page-symbolic");
                    } else {
                        southstar_gtk_ffi::gtk_image_set_from_paintable(icon.cast(), fav);
                    }
                    gtk::gtk_image_set_pixel_size(icon, 16);
                }
            }
            _ => {}
        }
    }
}

unsafe extern "C" fn on_tab_clicked(button: *mut GtkWidget, page: *mut c_void) {
    unsafe {
        let pw = pw(gtk::g_object_get_data(button.cast(), PW_KEY.as_ptr()));
        let idx = pw.page_index(page.cast());
        if idx >= 0 {
            gtk::gtk_notebook_set_current_page(pw.notebook(), idx);
        }
    }
}

unsafe extern "C" fn on_tab_close(button: *mut GtkWidget, page: *mut c_void) {
    unsafe {
        let pw = pw(gtk::g_object_get_data(button.cast(), PW_KEY.as_ptr()));
        pw.close_page(page.cast());
    }
}

unsafe extern "C" fn address_select_all_idle(data: *mut c_void) -> GBoolean {
    unsafe {
        let address = pw(data).address.get();
        if !address.is_null()
            && gtk::g_type_check_instance_is_a(address.cast(), gtk::gtk_editable_get_type()) != 0
        {
            gtk::gtk_editable_select_region(address, 0, -1);
        }
    }
    gtk::SOURCE_REMOVE
}

unsafe extern "C" fn on_address_focus_enter(_ctrl: *mut c_void, data: *mut c_void) {
    unsafe { gtk::g_idle_add(address_select_all_idle, data) };
}

unsafe extern "C" fn address_click_select_all_idle(data: *mut c_void) -> GBoolean {
    unsafe {
        let address = pw(data).address.get();
        if !address.is_null()
            && gtk::g_type_check_instance_is_a(address.cast(), gtk::gtk_editable_get_type()) != 0
            && gtk::gtk_editable_get_selection_bounds(address, ptr::null_mut(), ptr::null_mut())
                == 0
        {
            gtk::gtk_editable_select_region(address, 0, -1);
        }
    }
    gtk::SOURCE_REMOVE
}

unsafe extern "C" fn on_address_click_pressed(
    _gesture: *mut c_void,
    n_press: c_int,
    _x: c_double,
    _y: c_double,
    data: *mut c_void,
) {
    let pw = unsafe { pw(data) };
    let focused = unsafe { gtk::gtk_widget_get_state_flags(pw.address.get()) }
        & gtk::STATE_FLAG_FOCUS_WITHIN
        != 0;
    pw.address_click_focuses.set(n_press == 1 && !focused);
}

unsafe extern "C" fn on_address_click_released(
    _gesture: *mut c_void,
    _n_press: c_int,
    _x: c_double,
    _y: c_double,
    data: *mut c_void,
) {
    let pw = unsafe { pw(data) };
    if !pw.address_click_focuses.get() {
        return;
    }
    pw.address_click_focuses.set(false);
    unsafe { gtk::g_idle_add(address_click_select_all_idle, data) };
}

unsafe extern "C" fn on_address_activate(_entry: *mut GtkWidget, data: *mut c_void) {
    unsafe { pw(data) }.activate_address();
}

unsafe extern "C" fn on_address_key_pressed(
    _controller: *mut c_void,
    keyval: c_uint,
    _keycode: c_uint,
    state: c_uint,
    data: *mut c_void,
) -> GBoolean {
    let pw = unsafe { pw(data) };
    if keyval != gtk::KEY_RETURN && keyval != gtk::KEY_KP_ENTER {
        return 0;
    }
    let ctrl = state & gtk::CONTROL_MASK != 0;
    let shift = state & gtk::SHIFT_MASK != 0;
    if !ctrl && !shift {
        return 0;
    }
    let Some(host) = rules::completed_host(pw.address_text().to_bytes(), ctrl, shift) else {
        return 0;
    };
    let host = cstring(host);
    unsafe { gtk::gtk_editable_set_text(pw.address.get(), host.as_ptr()) };
    pw.activate_address();
    1
}

unsafe extern "C" fn on_back_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    if let Some(v) = unsafe { pw(data) }.current_view() {
        v.back();
    }
}

unsafe extern "C" fn on_forward_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    if let Some(v) = unsafe { pw(data) }.current_view() {
        v.forward();
    }
}

unsafe extern "C" fn on_reload_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    let Some(v) = pw.current_view() else {
        return;
    };
    if pw.loading.get() {
        v.stop();
    } else {
        v.reload();
    }
}

fn load_home_page(v: View) {
    let home = {
        let config = southstar_config::lock();
        config.reload();
        // SAFETY: home_url is null or a NUL-terminated string the config owns while it stays locked.
        let home = unsafe { borrowed(config.get().home_url) }
            .filter(|h| !h.is_empty())
            .unwrap_or(c"about:start");
        normalize_url(home)
    };
    v.load(&home);
}

unsafe extern "C" fn on_home_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    if let Some(v) = unsafe { pw(data) }.current_view() {
        load_home_page(v);
    }
}

unsafe extern "C" fn on_logo_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    unsafe { pw(data) }.load_in_current(rules::PROJECT_URL);
}

unsafe extern "C" fn on_address_icon_press(_entry: *mut GtkWidget, pos: c_int, data: *mut c_void) {
    if pos == gtk::ENTRY_ICON_SECONDARY {
        unsafe {
            gtk::g_action_group_activate_action(
                pw(data).window.get().cast(),
                c"bookmark-page".as_ptr(),
                ptr::null_mut(),
            )
        };
    }
}

unsafe extern "C" fn on_downloads_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    unsafe { downloads::show(pw(data).window.get()) };
}

unsafe extern "C" fn on_newtab_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    unsafe { pw(data) }.add_tab(c"about:start", true);
}

unsafe extern "C" fn on_switch_page(
    _nb: *mut GtkWidget,
    page: *mut GtkWidget,
    _num: c_uint,
    data: *mut c_void,
) {
    let pw = unsafe { pw(data) };
    if pw.element_fullscreen.get() && unsafe { view::for_page(page) } != pw.fullscreen_view.get() {
        pw.leave_element_fullscreen();
    }
    pw.update_active_tab();
    pw.update_chrome();
}

type Action = unsafe extern "C" fn(action: *mut c_void, parameter: *mut c_void, data: *mut c_void);

macro_rules! view_action {
    ($name:ident, |$v:ident| $body:expr) => {
        unsafe extern "C" fn $name(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
            if let Some($v) = unsafe { pw(data) }.current_view() {
                $body;
            }
        }
    };
}

view_action!(act_back, |v| v.back());
view_action!(act_forward, |v| v.forward());
view_action!(act_reload, |v| v.reload());
view_action!(act_find, |v| v.find_open());
view_action!(act_console, |v| v.toggle_console());
view_action!(act_home, |v| load_home_page(v));
view_action!(act_zoom_in, |v| v.zoom_in());
view_action!(act_zoom_out, |v| v.zoom_out());
view_action!(act_zoom_reset, |v| v.zoom_reset());
view_action!(act_about, |v| v.load(c"about:southstar"));
view_action!(act_settings, |v| v.load(c"about:settings"));
view_action!(act_history, |v| v.load(c"about:history"));

unsafe extern "C" fn act_hard_reload(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    let Some(v) = unsafe { pw(data) }.current_view() else {
        return;
    };
    southstar_http_cache::clear();
    v.reload();
}

unsafe extern "C" fn act_bookmark_page(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    unsafe { pw(data) }.toggle_bookmark();
}

unsafe extern "C" fn act_view_source(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    let Some(v) = unsafe { pw(data) }.current_view() else {
        return;
    };
    let Some(url) = v.url() else {
        return;
    };
    if url.is_empty() || url.to_bytes().starts_with(b"view-source:") {
        return;
    }
    let src = cstring([b"view-source:".as_slice(), url.to_bytes()].concat());
    v.load(&src);
}

unsafe extern "C" fn act_new_tab(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    unsafe { pw(data) }.add_tab(c"about:start", true);
}

unsafe extern "C" fn act_new_private_tab(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    unsafe { pw(data) }.add_tab_full(c"about:start", true, true);
}

unsafe extern "C" fn act_close_tab(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    let idx = unsafe { gtk::gtk_notebook_get_current_page(pw.notebook()) };
    if idx < 0 {
        return;
    }
    pw.close_page(pw.nth_page(idx));
}

unsafe extern "C" fn act_focus_address(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    unsafe {
        gtk::gtk_widget_grab_focus(pw.address.get());
        gtk::gtk_editable_select_region(pw.address.get(), 0, -1);
    }
}

unsafe extern "C" fn act_focus_page(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    let v = pw.current_view();
    let address = pw.address.get();
    let focus = unsafe { gtk::gtk_window_get_focus(pw.window.get()) };
    let editing_address = !focus.is_null()
        && !address.is_null()
        && (focus == address || unsafe { gtk::gtk_widget_is_ancestor(focus, address) } != 0);
    if editing_address {
        let url = v.and_then(View::url);
        pw.set_address_text(Some(url.as_deref().unwrap_or(c"")));
    } else if view::find_close(v) {
        return;
    } else if let Some(v) = v.filter(|v| v.is_loading()) {
        v.stop();
    }
    if let Some(v) = v {
        v.focus();
    }
}

fn step_tab(pw: &ProcWindow, delta: c_int) {
    unsafe {
        let n = gtk::gtk_notebook_get_n_pages(pw.notebook());
        if n <= 1 {
            return;
        }
        let idx = gtk::gtk_notebook_get_current_page(pw.notebook());
        gtk::gtk_notebook_set_current_page(pw.notebook(), rules::step_tab(idx, delta, n));
    }
}

unsafe extern "C" fn act_next_tab(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    step_tab(unsafe { pw(data) }, 1);
}

unsafe extern "C" fn act_prev_tab(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    step_tab(unsafe { pw(data) }, -1);
}

unsafe extern "C" fn act_quit(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    unsafe { gtk::gtk_window_close(pw(data).window.get()) };
}

unsafe extern "C" fn act_downloads(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    unsafe { downloads::show(pw(data).window.get()) };
}

unsafe extern "C" fn act_task_manager(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    unsafe { taskmgr::show(pw.window.get(), pw.notebook()) };
}

unsafe extern "C" fn act_print(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    view::print(unsafe { pw(data) }.current_view());
}

unsafe extern "C" fn act_save_pdf(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    view::save_pdf(unsafe { pw(data) }.current_view());
}

unsafe extern "C" fn act_save_image(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    view::save_image(unsafe { pw(data) }.current_view());
}

unsafe extern "C" fn act_fullscreen(_a: *mut c_void, _p: *mut c_void, data: *mut c_void) {
    unsafe { pw(data) }.toggle_window_fullscreen();
}

fn action_for(name: &CStr) -> Action {
    match name.to_bytes() {
        b"back" => act_back,
        b"forward" => act_forward,
        b"reload" => act_reload,
        b"hard-reload" => act_hard_reload,
        b"find" => act_find,
        b"console" => act_console,
        b"view-source" => act_view_source,
        b"bookmark-page" => act_bookmark_page,
        b"home" => act_home,
        b"new-tab" => act_new_tab,
        b"new-private-tab" => act_new_private_tab,
        b"close-tab" => act_close_tab,
        b"focus-address" => act_focus_address,
        b"focus-page" => act_focus_page,
        b"zoom-in" => act_zoom_in,
        b"zoom-out" => act_zoom_out,
        b"zoom-reset" => act_zoom_reset,
        b"next-tab" => act_next_tab,
        b"prev-tab" => act_prev_tab,
        b"task-manager" => act_task_manager,
        b"downloads" => act_downloads,
        b"about" => act_about,
        b"history" => act_history,
        b"print" => act_print,
        b"save-pdf" => act_save_pdf,
        b"save-image" => act_save_image,
        b"fullscreen" => act_fullscreen,
        b"settings" => act_settings,
        _ => act_quit,
    }
}

unsafe fn install_shortcuts(pw: &ProcWindow) {
    unsafe {
        let window = pw.window.get().cast::<c_void>();
        for shortcut in rules::SHORTCUTS {
            let act = gtk::g_simple_action_new(shortcut.name.as_ptr(), ptr::null());
            connect(
                act,
                c"activate",
                handler!(action_for(shortcut.name)),
                data(pw),
            );
            gtk::g_action_map_add_action(window, act);
            gtk::g_object_unref(act);
            if !shortcut.accels.is_empty() {
                let full = cstring([b"win.".as_slice(), shortcut.name.to_bytes()].concat());
                let mut accels: Vec<*const c_char> =
                    shortcut.accels.iter().map(|a| a.as_ptr()).collect();
                accels.push(ptr::null());
                gtk::gtk_application_set_accels_for_action(pw.app, full.as_ptr(), accels.as_ptr());
            }
        }
        if southstar_renderer_host::single_process_enabled() {
            for name in [c"new-tab", c"new-private-tab"] {
                let act = gtk::g_action_map_lookup_action(window, name.as_ptr());
                if !act.is_null() {
                    gtk::g_simple_action_set_enabled(act, 0);
                }
            }
        }
    }
}

unsafe extern "C" fn on_window_key_pressed(
    _controller: *mut c_void,
    keyval: c_uint,
    _keycode: c_uint,
    _state: c_uint,
    data: *mut c_void,
) -> GBoolean {
    let pw = unsafe { pw(data) };
    let win = pw.window.get();
    if pw.element_fullscreen.get() && (keyval == gtk::KEY_F11 || keyval == gtk::KEY_ESCAPE) {
        pw.leave_element_fullscreen();
        return 1;
    }
    unsafe {
        if keyval == gtk::KEY_F11 {
            pw.toggle_window_fullscreen();
            return 1;
        }
        if keyval == gtk::KEY_ESCAPE && gtk::gtk_window_is_fullscreen(win) != 0 {
            gtk::gtk_window_unfullscreen(win);
            gtk::gtk_window_maximize(win);
            return 1;
        }
    }
    0
}

unsafe fn build_menu() -> *mut c_void {
    unsafe {
        let appmenu = gtk::g_menu_new();
        for section in rules::MENU {
            let sec = gtk::g_menu_new();
            for item in *section {
                let mi = gtk::g_menu_item_new(tr(item.label).as_ptr(), item.action.as_ptr());
                if let Some(accel) = item.accel {
                    gtk::g_menu_item_set_attribute_value(
                        mi,
                        c"accel".as_ptr(),
                        gtk::g_variant_new_string(accel.as_ptr()),
                    );
                }
                gtk::g_menu_append_item(sec, mi);
                gtk::g_object_unref(mi);
            }
            gtk::g_menu_append_section(appmenu, ptr::null(), sec);
            gtk::g_object_unref(sec);
        }
        let about = gtk::g_menu_new();
        gtk::g_menu_append(
            about,
            tr(c"About Southstar").as_ptr(),
            c"win.about".as_ptr(),
        );
        gtk::g_menu_append_section(appmenu, ptr::null(), about);
        gtk::g_object_unref(about);
        appmenu
    }
}

unsafe extern "C" fn on_bookmark_activate(button: *mut GtkWidget, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    unsafe {
        let url = borrowed(gtk::g_object_get_data(button.cast(), BOOKMARK_URL_KEY.as_ptr()).cast());
        if let (Some(url), Some(v)) = (url, pw.current_view()) {
            v.load(url);
        }
        let pop = southstar_gtk_ffi::gtk_widget_get_ancestor(button, gtk::gtk_popover_get_type());
        if !pop.is_null() {
            gtk::gtk_popover_popdown(pop);
        }
    }
}

unsafe extern "C" fn on_bookmark_remove(button: *mut GtkWidget, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    unsafe {
        let url = gtk::g_object_get_data(button.cast(), BOOKMARK_URL_KEY.as_ptr()).cast::<c_char>();
        if url.is_null() {
            return;
        }
        pw.bookmarks.borrow_mut().remove(CStr::from_ptr(url));
        let row = gtk::gtk_widget_get_parent(button);
        let list = if row.is_null() {
            ptr::null_mut()
        } else {
            gtk::gtk_widget_get_parent(row)
        };
        if !list.is_null() && !row.is_null() {
            gtk::gtk_box_remove(list, row);
        }
        pw.update_bookmark_indicator();
    }
}

unsafe extern "C" fn on_add_bookmark(button: *mut GtkWidget, data: *mut c_void) {
    let pw = unsafe { pw(data) };
    if !pw.toggle_bookmark() {
        return;
    }
    unsafe {
        let pop = southstar_gtk_ffi::gtk_widget_get_ancestor(button, gtk::gtk_popover_get_type());
        if !pop.is_null() {
            gtk::gtk_popover_popdown(pop);
        }
    }
}

unsafe fn bookmark_button(label: &CStr, url: &CStr) -> *mut GtkWidget {
    unsafe {
        let b = gtk::gtk_button_new_with_label(label.as_ptr());
        gtk::g_object_set_data_full(
            b.cast(),
            BOOKMARK_URL_KEY.as_ptr(),
            gtk::g_strdup(url.as_ptr()).cast(),
            Some(gtk::g_free),
        );
        b
    }
}

unsafe fn build_bookmarks_popover(pw: &ProcWindow) -> *mut GtkWidget {
    unsafe {
        let vbox = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 4);
        set_margins(vbox, 6, 6, 6, 6);
        gtk::gtk_widget_set_size_request(vbox, 320, -1);

        let add = gtk::gtk_button_new_with_label(
            if pw.current_page_bookmarked() {
                tr(c"Remove this bookmark")
            } else {
                tr(c"Bookmark this page")
            }
            .as_ptr(),
        );
        connect(add, c"clicked", handler!(on_add_bookmark), data(pw));
        gtk::gtk_box_append(vbox, add);
        gtk::gtk_box_append(vbox, gtk::gtk_separator_new(gtk::ORIENTATION_HORIZONTAL));

        let scroll = gtk::gtk_scrolled_window_new();
        gtk::gtk_scrolled_window_set_policy(scroll, gtk::POLICY_NEVER, gtk::POLICY_AUTOMATIC);
        gtk::gtk_scrolled_window_set_propagate_natural_height(scroll, 1);
        gtk::gtk_scrolled_window_set_max_content_height(scroll, 320);
        let list = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 2);
        let saved: Vec<(CString, CString)> = pw
            .bookmarks
            .borrow()
            .items()
            .iter()
            .map(|b| (b.url().to_owned(), b.title().to_owned()))
            .collect();
        if saved.is_empty() {
            let empty = gtk::gtk_label_new(tr(c"No bookmarks yet").as_ptr());
            gtk::gtk_widget_add_css_class(empty, c"dim-label".as_ptr());
            gtk::gtk_widget_set_margin_top(empty, 12);
            gtk::gtk_widget_set_margin_bottom(empty, 12);
            gtk::gtk_box_append(list, empty);
        }
        for (url, title) in &saved {
            let url = url.as_c_str();
            let title = Some(title.as_c_str())
                .filter(|t| !t.is_empty())
                .unwrap_or(url);
            let row = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 4);
            let open = bookmark_button(title, url);
            gtk::gtk_button_set_has_frame(open, 0);
            gtk::gtk_widget_set_hexpand(open, 1);
            gtk::gtk_widget_set_halign(open, gtk::ALIGN_START);
            gtk::gtk_widget_set_tooltip_text(open, url.as_ptr());
            connect(open, c"clicked", handler!(on_bookmark_activate), data(pw));
            let del = gtk::gtk_button_new_from_icon_name(c"user-trash-symbolic".as_ptr());
            gtk::gtk_button_set_has_frame(del, 0);
            gtk::gtk_widget_set_tooltip_text(del, tr(c"Remove bookmark").as_ptr());
            set_accessible_label(del, tr(c"Remove bookmark"));
            gtk::g_object_set_data_full(
                del.cast(),
                BOOKMARK_URL_KEY.as_ptr(),
                gtk::g_strdup(url.as_ptr()).cast(),
                Some(gtk::g_free),
            );
            connect(del, c"clicked", handler!(on_bookmark_remove), data(pw));
            gtk::gtk_box_append(row, open);
            gtk::gtk_box_append(row, del);
            gtk::gtk_box_append(list, row);
        }
        gtk::gtk_scrolled_window_set_child(scroll, list);
        gtk::gtk_box_append(vbox, scroll);

        let pop = gtk::gtk_popover_new();
        gtk::gtk_popover_set_child(pop, vbox);
        pop
    }
}

unsafe extern "C" fn popover_unparent_idle(pop: *mut c_void) -> GBoolean {
    unsafe {
        let pop = pop.cast::<GtkWidget>();
        if !gtk::gtk_widget_get_parent(pop).is_null() {
            gtk::gtk_widget_unparent(pop);
        }
        gtk::g_object_unref(pop.cast());
    }
    gtk::SOURCE_REMOVE
}

unsafe extern "C" fn on_popover_closed(pop: *mut GtkWidget, _data: *mut c_void) {
    unsafe { gtk::g_idle_add(popover_unparent_idle, gtk::g_object_ref(pop.cast())) };
}

unsafe extern "C" fn on_bookmarks_clicked(button: *mut GtkWidget, data: *mut c_void) {
    unsafe {
        let pop = build_bookmarks_popover(pw(data));
        southstar_gtk_ffi::gtk_widget_set_parent(pop, button);
        gtk::gtk_popover_set_position(pop, gtk::POS_BOTTOM);
        connect(pop, c"closed", handler!(on_popover_closed), ptr::null_mut());
        gtk::gtk_popover_popup(pop);
    }
}

unsafe extern "C" fn on_zoom_indicator_clicked(_b: *mut GtkWidget, data: *mut c_void) {
    if let Some(v) = unsafe { pw(data) }.current_view() {
        v.zoom_reset();
    }
}

unsafe extern "C" fn on_theme_changed(
    _settings: *mut c_void,
    _pspec: *mut c_void,
    data: *mut c_void,
) {
    unsafe { pw(data) }.apply_color_scheme();
}

unsafe fn build_address(pw: &ProcWindow) -> *mut GtkWidget {
    unsafe {
        let address = gtk::gtk_entry_new();
        gtk::gtk_widget_set_hexpand(address, 1);
        gtk::gtk_widget_set_valign(address, gtk::ALIGN_CENTER);
        gtk::gtk_widget_add_css_class(address, c"ns-address".as_ptr());
        ns_icon_entry_set(
            address.cast(),
            gtk::ENTRY_ICON_PRIMARY,
            c"southstar-globe-symbolic",
        );
        gtk::gtk_entry_set_icon_tooltip_text(
            address,
            gtk::ENTRY_ICON_PRIMARY,
            tr(c"Page location").as_ptr(),
        );
        gtk::gtk_entry_set_placeholder_text(address, tr(c"Search or enter a URL").as_ptr());
        set_accessible_label(address, tr(c"Address and search bar"));
        connect(
            address,
            c"activate",
            handler!(on_address_activate),
            data(pw),
        );
        connect(
            address,
            c"icon-press",
            handler!(on_address_icon_press),
            data(pw),
        );
        let focus = gtk::gtk_event_controller_focus_new();
        connect(focus, c"enter", handler!(on_address_focus_enter), data(pw));
        gtk::gtk_widget_add_controller(address, focus);
        let click = gtk::gtk_gesture_click_new();
        gtk::gtk_event_controller_set_propagation_phase(click, gtk::PHASE_CAPTURE);
        connect(
            click,
            c"pressed",
            handler!(on_address_click_pressed),
            data(pw),
        );
        connect(
            click,
            c"released",
            handler!(on_address_click_released),
            data(pw),
        );
        gtk::gtk_widget_add_controller(address, click);
        let keys = gtk::gtk_event_controller_key_new();
        gtk::gtk_event_controller_set_propagation_phase(keys, gtk::PHASE_CAPTURE);
        connect(
            keys,
            c"key-pressed",
            handler!(on_address_key_pressed),
            data(pw),
        );
        gtk::gtk_widget_add_controller(address, keys);
        address
    }
}

unsafe fn page_label(class: &CStr, halign: c_int, valign: c_int) -> *mut GtkWidget {
    unsafe {
        let l = gtk::gtk_label_new(c"".as_ptr());
        gtk::gtk_label_set_ellipsize(l, gtk::ELLIPSIZE_MIDDLE);
        gtk::gtk_label_set_max_width_chars(l, 90);
        gtk::gtk_widget_add_css_class(l, class.as_ptr());
        gtk::gtk_widget_set_halign(l, halign);
        gtk::gtk_widget_set_valign(l, valign);
        southstar_gtk_ffi::gtk_widget_set_can_target(l, 0);
        gtk::gtk_widget_set_visible(l, 0);
        l
    }
}

pub unsafe fn proc_window_new(
    app: *mut c_void,
    initial_size: Option<(c_int, c_int)>,
) -> &'static ProcWindow {
    unsafe {
        let pw: &'static ProcWindow = &*Box::into_raw(Box::new(ProcWindow {
            app,
            window: Cell::new(ptr::null_mut()),
            header: Cell::new(ptr::null_mut()),
            toolbar: Cell::new(ptr::null_mut()),
            notebook: Cell::new(ptr::null_mut()),
            tabstrip: Cell::new(ptr::null_mut()),
            newtab_btn: Cell::new(ptr::null_mut()),
            address: Cell::new(ptr::null_mut()),
            address_click_focuses: Cell::new(false),
            zoom_button: Cell::new(ptr::null_mut()),
            back: Cell::new(ptr::null_mut()),
            forward: Cell::new(ptr::null_mut()),
            reload: Cell::new(ptr::null_mut()),
            loading: Cell::new(false),
            status: Cell::new(ptr::null_mut()),
            status_base: RefCell::new(CString::default()),
            status_timer: Cell::new(0),
            theme_watch: Cell::new([0; 2]),
            webgl_active: Cell::new(false),
            element_fullscreen: Cell::new(false),
            fullscreen_view: Cell::new(None),
            fullscreen_notice: Cell::new(ptr::null_mut()),
            fullscreen_notice_timer: Cell::new(0),
            bookmarks: RefCell::new(Bookmarks::load()),
            session_path: RefCell::new(None),
            session_timer: Cell::new(0),
        }));
        let window = gtk::gtk_application_window_new(app);
        pw.window.set(window);
        gtk::g_object_set_data_full(
            window.cast(),
            WINDOW_KEY.as_ptr(),
            data(pw),
            Some(procwindow_free),
        );
        gtk::gtk_window_set_title(window, brand_versioned().as_ptr());
        match initial_size {
            Some((w, h)) => gtk::gtk_window_set_default_size(window, w, h),
            None => {
                gtk::gtk_window_set_default_size(window, 1024, 768);
                gtk::gtk_window_maximize(window);
            }
        }

        let winkeys = gtk::gtk_event_controller_key_new();
        gtk::gtk_event_controller_set_propagation_phase(winkeys, gtk::PHASE_CAPTURE);
        connect(
            winkeys,
            c"key-pressed",
            handler!(on_window_key_pressed),
            data(pw),
        );
        gtk::gtk_widget_add_controller(window, winkeys);

        let header = gtk::gtk_header_bar_new();
        pw.header.set(header);
        gtk::gtk_header_bar_set_show_title_buttons(header, 0);
        let tabstrip = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 2);
        pw.tabstrip.set(tabstrip);
        gtk::gtk_widget_add_css_class(tabstrip, c"ns-tabstrip".as_ptr());
        let newtab = ns_icon_button_new(c"southstar-new-tab-symbolic");
        pw.newtab_btn.set(newtab);
        gtk::gtk_button_set_has_frame(newtab, 0);
        gtk::gtk_widget_add_css_class(newtab, c"ns-newtab".as_ptr());
        gtk::gtk_widget_set_tooltip_text(newtab, tr(c"New tab").as_ptr());
        set_accessible_label(newtab, tr(c"New tab"));
        connect(newtab, c"clicked", handler!(on_newtab_clicked), data(pw));
        gtk::gtk_widget_set_valign(newtab, gtk::ALIGN_CENTER);
        gtk::gtk_box_append(tabstrip, newtab);
        if southstar_renderer_host::single_process_enabled() {
            gtk::gtk_widget_set_visible(newtab, 0);
        }
        gtk::gtk_header_bar_set_title_widget(header, ns_title_row_new(tabstrip));
        gtk::gtk_window_set_titlebar(window, header);

        let vbox = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 0);
        let toolbar = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 2);
        pw.toolbar.set(toolbar);
        gtk::gtk_widget_add_css_class(toolbar, c"ns-toolbar".as_ptr());

        pw.back.set(toolbar_button(
            c"southstar-back-symbolic",
            tr(c"Go back one page"),
            handler!(on_back_clicked),
            pw,
        ));
        pw.forward.set(toolbar_button(
            c"southstar-forward-symbolic",
            tr(c"Go forward one page"),
            handler!(on_forward_clicked),
            pw,
        ));
        pw.reload.set(toolbar_button(
            c"southstar-reload-symbolic",
            tr(c"Reload this page"),
            handler!(on_reload_clicked),
            pw,
        ));
        let home = toolbar_button(
            c"southstar-home-symbolic",
            tr(c"Go to the home page"),
            handler!(on_home_clicked),
            pw,
        );
        let downloads_btn = toolbar_button(
            c"southstar-downloads-symbolic",
            tr(c"Show downloads"),
            handler!(on_downloads_clicked),
            pw,
        );

        pw.address.set(build_address(pw));

        let zoom = gtk::gtk_button_new_with_label(c"100%".as_ptr());
        pw.zoom_button.set(zoom);
        gtk::gtk_button_set_has_frame(zoom, 0);
        gtk::gtk_widget_add_css_class(zoom, c"ns-zoom".as_ptr());
        gtk::gtk_widget_set_valign(zoom, gtk::ALIGN_CENTER);
        gtk::gtk_widget_set_tooltip_text(zoom, tr(c"Reset zoom (Ctrl+0)").as_ptr());
        set_accessible_label(zoom, tr(c"Reset zoom"));
        gtk::gtk_widget_set_visible(zoom, 0);
        connect(
            zoom,
            c"clicked",
            handler!(on_zoom_indicator_clicked),
            data(pw),
        );

        let bookmarks_button = toolbar_button(
            c"southstar-bookmarks-symbolic",
            tr(c"Bookmarks"),
            handler!(on_bookmarks_clicked),
            pw,
        );
        let appmenu = build_menu();
        let menu_button = gtk::gtk_menu_button_new();
        ns_icon_menu_button_set(menu_button.cast(), c"southstar-menu-symbolic");
        gtk::gtk_widget_add_css_class(menu_button, c"ns-nav-button".as_ptr());
        gtk::gtk_widget_set_valign(menu_button, gtk::ALIGN_CENTER);
        gtk::gtk_menu_button_set_menu_model(menu_button, appmenu);
        ns_popover_menu_fit(gtk::gtk_menu_button_get_popover(menu_button));
        gtk::gtk_widget_set_tooltip_text(menu_button, tr(c"Menu").as_ptr());
        set_accessible_label(menu_button, tr(c"Menu"));
        gtk::g_object_unref(appmenu);

        let logo = toolbar_button(
            c"southstar",
            tr(c"Visit the project page"),
            handler!(on_logo_clicked),
            pw,
        );
        gtk::gtk_widget_add_css_class(logo, c"ns-logo".as_ptr());

        for w in [
            pw.back.get(),
            pw.forward.get(),
            pw.reload.get(),
            home,
            pw.address.get(),
            zoom,
            bookmarks_button,
            downloads_btn,
            menu_button,
            logo,
        ] {
            gtk::gtk_box_append(toolbar, w);
        }
        gtk::gtk_box_append(vbox, toolbar);

        let notebook = gtk::gtk_notebook_new();
        pw.notebook.set(notebook);
        gtk::gtk_notebook_set_show_tabs(notebook, 0);
        gtk::gtk_notebook_set_show_border(notebook, 0);
        gtk::gtk_notebook_set_scrollable(notebook, 1);
        gtk::gtk_widget_set_hexpand(notebook, 1);
        gtk::gtk_widget_set_vexpand(notebook, 1);
        gtk::g_signal_connect_data(
            notebook.cast(),
            c"switch-page".as_ptr(),
            handler!(on_switch_page),
            data(pw),
            None,
            gtk::CONNECT_AFTER,
        );

        pw.status.set(page_label(
            c"ns-procstatus",
            gtk::ALIGN_START,
            gtk::ALIGN_END,
        ));
        pw.fullscreen_notice.set(page_label(
            c"ns-fullscreen-notice",
            gtk::ALIGN_CENTER,
            gtk::ALIGN_START,
        ));

        let overlay = gtk::gtk_overlay_new();
        gtk::gtk_overlay_set_child(overlay, notebook);
        gtk::gtk_overlay_add_overlay(overlay, pw.status.get());
        gtk::gtk_overlay_add_overlay(overlay, pw.fullscreen_notice.get());
        gtk::gtk_widget_set_hexpand(overlay, 1);
        gtk::gtk_widget_set_vexpand(overlay, 1);
        gtk::gtk_box_append(vbox, overlay);

        gtk::gtk_window_set_child(window, vbox);
        install_shortcuts(pw);

        let settings = gtk::gtk_settings_get_default();
        if !settings.is_null() {
            pw.theme_watch.set([
                connect(
                    settings,
                    c"notify::gtk-theme-name",
                    handler!(on_theme_changed),
                    data(pw),
                ),
                connect(
                    settings,
                    c"notify::gtk-application-prefer-dark-theme",
                    handler!(on_theme_changed),
                    data(pw),
                ),
            ]);
        }
        pw
    }
}

pub unsafe extern "C" fn write_session(data: *mut c_void) -> GBoolean {
    let pw = unsafe { pw(data) };
    let path = pw.session_path.borrow().clone();
    let Some(path) = path else {
        return gtk::SOURCE_REMOVE;
    };
    let mut s: Vec<u8> = Vec::new();
    for v in unsafe { view::views(pw.notebook()) } {
        if v.is_private() {
            continue;
        }
        if let Some(u) = v
            .url()
            .filter(|u| rules::session_url_recoverable(u.to_bytes()))
        {
            s.extend_from_slice(u.to_bytes());
            s.push(b'\n');
        }
    }
    unsafe {
        gtk::g_file_set_contents(
            path.as_ptr(),
            s.as_ptr().cast(),
            s.len() as isize,
            ptr::null_mut(),
        )
    };
    gtk::SOURCE_CONTINUE
}

impl ProcWindow {
    pub fn present(&self) {
        unsafe { gtk::gtk_window_present(self.window.get()) };
    }

    pub fn apply_scheme(&self) {
        self.apply_color_scheme();
    }

    pub fn status(&self, text: &CStr) {
        self.set_status(text);
    }

    pub fn as_data(&self) -> *mut c_void {
        data(self)
    }
}
