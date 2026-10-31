//! Southstar — the tabbed browser window: tab strip, toolbar, address bar, status pill, actions, menu and bookmarks popover.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ops::ControlFlow;
use core::ptr;
use std::ffi::CString;
use std::rc::{Rc, Weak};
use std::sync::{Arc, OnceLock};

use southstar_bookmarks::Bookmarks;
use southstar_config::NsConfig;
use southstar_glib::{self as glib, SourceId};
use southstar_gtk::{self as gtk, Object, SignalHandlerId, Widget};
use southstar_gtk_ffi as gtk_sys;

use super::appmain::setenv;
use super::gtk::{borrowed, cstring, tr, tr_string};
use super::icons;
use super::procview_chrome::ns_popover_menu_fit;
use super::titlerow::ns_title_row_new;
use super::view::{self, View};
use super::{downloads, taskmgr};
use crate::window as rules;

const WINDOW_KEY: &CStr = c"ns-procwindow";
const PW_KEY: &CStr = c"ns-pw";

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

struct Tab {
    page: Widget,
    wrapper: gtk::Box,
    close: gtk::Button,
    label: gtk::Label,
    icon: gtk::Image,
    spinner: gtk::Spinner,
}

pub struct ProcWindow {
    me: Weak<ProcWindow>,
    window: gtk::WeakWidget,
    header: gtk::HeaderBar,
    toolbar: gtk::Box,
    notebook: gtk::Notebook,
    tabstrip: gtk::Box,
    newtab_btn: gtk::Button,
    tabs: RefCell<Vec<Tab>>,
    address: gtk::Entry,
    address_click_focuses: Cell<bool>,
    zoom_button: gtk::Button,
    back: gtk::Button,
    forward: gtk::Button,
    reload: gtk::Button,
    loading: Cell<bool>,
    status_label: gtk::Label,
    status_base: RefCell<CString>,
    status_timer: Cell<Option<SourceId>>,
    theme_watch: RefCell<Option<(Object, [SignalHandlerId; 2])>>,
    webgl_active: Cell<bool>,
    element_fullscreen: Cell<bool>,
    fullscreen_view: Cell<Option<View>>,
    fullscreen_notice: gtk::Label,
    fullscreen_notice_timer: Cell<Option<SourceId>>,
    bookmarks: RefCell<Bookmarks>,
    pub session_path: RefCell<Option<CString>>,
    session_timer: Cell<Option<SourceId>>,
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

fn config() -> Option<Arc<NsConfig>> {
    southstar_config::get()
}

fn on(me: &Weak<ProcWindow>, f: impl Fn(&ProcWindow) + 'static) -> impl Fn() + 'static {
    let me = me.clone();
    move || {
        if let Some(pw) = me.upgrade() {
            f(&pw);
        }
    }
}

fn nav_button(icon: &CStr, tooltip: &CStr) -> gtk::Button {
    let b = icons::button_new(icon);
    b.set_has_frame(false);
    b.add_css_class(c"ns-nav-button");
    b.set_valign(gtk_sys::ALIGN_CENTER);
    b.set_tooltip_text(Some(tooltip));
    b.set_accessible_label(tooltip);
    b
}

fn toolbar_button(
    me: &Weak<ProcWindow>,
    icon: &CStr,
    tooltip: &CStr,
    f: impl Fn(&ProcWindow) + 'static,
) -> gtk::Button {
    let b = nav_button(icon, tooltip);
    b.connect_clicked(on(me, f));
    b
}

impl Drop for ProcWindow {
    fn drop(&mut self) {
        for timer in [
            &self.session_timer,
            &self.status_timer,
            &self.fullscreen_notice_timer,
        ] {
            if let Some(id) = timer.take() {
                id.remove();
            }
        }
        if let Some((settings, ids)) = self.theme_watch.take() {
            for id in ids {
                id.disconnect(&settings);
            }
        }
    }
}

impl ProcWindow {
    fn window_widget(&self) -> Option<Widget> {
        self.window.upgrade()
    }

    fn window(&self) -> Option<gtk::Window> {
        gtk::Window::cast(&self.window_widget()?)
    }

    fn current_view(&self) -> Option<View> {
        view::for_page(&self.notebook.current()?)
    }

    fn views(&self) -> Vec<View> {
        self.notebook
            .pages()
            .filter_map(|page| view::for_page(&page))
            .collect()
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
        icons::button_set(&self.reload, icon);
        self.reload.set_tooltip_text(Some(tip));
        self.reload.set_accessible_label(tip);
    }

    fn set_address_text(&self, url: Option<&CStr>) {
        self.address.set_text(&address_display_url(url));
    }

    fn update_security_indicator(&self, v: Option<View>) {
        let entry = &self.address;
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
        entry.set_css_class(c"ns-insecure", has_url && sec == rules::SECURITY_INVALID);
        let (Some(icon_name), Some(label), true) = (icon_name, label, has_url) else {
            let internal = url
                .as_deref()
                .is_some_and(|u| u.to_bytes().starts_with(b"about:"));
            let page_icon = if internal {
                c"southstar"
            } else {
                c"southstar-globe-symbolic"
            };
            icons::entry_set(entry, gtk_sys::ENTRY_ICON_PRIMARY, page_icon);
            entry.set_icon_activatable(gtk_sys::ENTRY_ICON_PRIMARY, false);
            entry.set_icon_tooltip_text(gtk_sys::ENTRY_ICON_PRIMARY, tr(c"Page location"));
            return;
        };
        icons::entry_set(entry, gtk_sys::ENTRY_ICON_PRIMARY, icon_name);
        entry.set_icon_activatable(gtk_sys::ENTRY_ICON_PRIMARY, false);
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
        entry.set_icon_tooltip_text(gtk_sys::ENTRY_ICON_PRIMARY, &cstring(tip));
    }

    fn apply_color_scheme(&self) {
        let dark = match config().map_or(0, |c| c.color_scheme) {
            COLOR_SCHEME_LIGHT => false,
            COLOR_SCHEME_DARK => true,
            _ => {
                let fg = self.window_widget().map(|w| w.color()).unwrap_or_default();
                rules::prefers_dark(fg.red, fg.green, fg.blue)
            }
        };
        let want = if dark { c"dark" } else { c"light" };
        if southstar_glib::env_bytes("NS_COLOR_SCHEME").as_deref() == Some(want.to_bytes()) {
            return;
        }
        setenv(c"NS_COLOR_SCHEME", want);
        southstar_browser::set_color_scheme(dark);
        for v in self.views() {
            v.set_color_scheme(dark);
            if v.url().is_some_and(|u| !u.is_empty()) {
                v.reload();
            }
        }
    }

    fn update_zoom_indicator(&self, v: Option<View>) {
        let percent = v.map_or(100, View::zoom_percent);
        if percent == 100 {
            self.zoom_button.set_visible(false);
            return;
        }
        self.zoom_button.set_label(&cstring(format!("{percent}%")));
        self.zoom_button.set_visible(true);
    }

    fn current_page_bookmarked(&self) -> bool {
        let url = self.current_view().and_then(View::url);
        url.is_some_and(|url| !url.is_empty() && self.bookmarks.borrow().contains(&url))
    }

    fn update_bookmark_indicator(&self) {
        let entry = &self.address;
        let url = self.current_view().and_then(View::url);
        let saved = self.current_page_bookmarked();
        entry.set_css_class(c"ns-bookmarked", saved);
        if url.is_none_or(|u| u.is_empty()) {
            entry.clear_icon(gtk_sys::ENTRY_ICON_SECONDARY);
            return;
        }
        let star = if saved {
            c"southstar-star-filled-symbolic"
        } else {
            c"southstar-star-symbolic"
        };
        icons::entry_set(entry, gtk_sys::ENTRY_ICON_SECONDARY, star);
        let tip = if saved {
            tr(c"Remove this bookmark")
        } else {
            tr(c"Bookmark this page")
        };
        entry.set_icon_tooltip_text(gtk_sys::ENTRY_ICON_SECONDARY, tip);
    }

    fn update_navigation(&self, v: View) {
        self.back.set_sensitive(v.can_back());
        self.forward.set_sensitive(v.can_forward());
    }

    fn update_chrome(&self) {
        let window = self.window();
        let Some(v) = self.current_view() else {
            self.address.set_text(c"");
            if let Some(window) = &window {
                window.set_title(brand_versioned());
            }
            self.back.set_sensitive(false);
            self.forward.set_sensitive(false);
            self.update_security_indicator(None);
            self.set_loading_ui(false);
            return;
        };
        self.set_loading_ui(v.is_loading());
        self.toolbar.set_css_class(c"ns-private", v.is_private());
        let url = v.url();
        let title = v.title();
        self.set_address_text(url.as_deref());
        let wt = cstring(rules::window_title(
            title.as_deref().map(CStr::to_bytes),
            brand_versioned().to_bytes(),
        ));
        if let Some(window) = &window {
            window.set_title(&wt);
        }
        self.update_navigation(v);
        self.update_security_indicator(Some(v));
        self.update_zoom_indicator(Some(v));
        self.update_bookmark_indicator();
    }

    fn render_status(&self) {
        let base = self.status_base.borrow().clone();
        let webgl = self.webgl_active.get();
        let note = tr(c"WebGL enabled");
        let text = cstring(rules::status_text(
            base.to_bytes(),
            webgl.then_some(note.to_bytes()),
        ));
        self.status_label.set_text(&text);
        self.status_label
            .set_visible(!self.element_fullscreen.get() && (!base.is_empty() || webgl));
    }

    fn set_status_persistent(&self, text: Option<&CStr>) {
        if let Some(timer) = self.status_timer.take() {
            timer.remove();
        }
        *self.status_base.borrow_mut() = text.unwrap_or(c"").to_owned();
        self.render_status();
    }

    fn set_status(&self, text: &CStr) {
        if let Some(timer) = self.status_timer.take() {
            timer.remove();
        }
        *self.status_base.borrow_mut() = text.to_owned();
        self.render_status();
        if text.is_empty() {
            return;
        }
        let me = self.me.clone();
        let timer = glib::timeout_add_seconds_local(5, move || {
            if let Some(pw) = me.upgrade() {
                pw.status_timer.set(None);
                *pw.status_base.borrow_mut() = CString::default();
                pw.render_status();
            }
            ControlFlow::Break(())
        });
        self.status_timer.set(Some(timer));
    }

    fn hide_fullscreen_notice(&self) {
        if let Some(timer) = self.fullscreen_notice_timer.take() {
            timer.remove();
        }
        self.fullscreen_notice.set_visible(false);
    }

    fn show_fullscreen_notice(&self, v: Option<View>) {
        let url = v.and_then(View::url);
        let host = url
            .and_then(|u| southstar_url::bytes::host_from(u.to_bytes()))
            .and_then(|host| CString::new(host).ok());
        let site = host
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| tr(c"This page").to_owned());
        let markup = cstring(format!(
            "<b>{}</b> {}",
            rules::markup_escape(&site.to_string_lossy()),
            rules::markup_escape(&tr_string(c"is now full screen — press Esc to exit")),
        ));
        self.fullscreen_notice.set_markup(&markup);
        self.fullscreen_notice.set_visible(true);
        if let Some(timer) = self.fullscreen_notice_timer.take() {
            timer.remove();
        }
        let me = self.me.clone();
        let timer = glib::timeout_add_seconds_local(rules::FULLSCREEN_NOTICE_SECONDS, move || {
            if let Some(pw) = me.upgrade() {
                pw.fullscreen_notice_timer.set(None);
                pw.fullscreen_notice.set_visible(false);
            }
            ControlFlow::Break(())
        });
        self.fullscreen_notice_timer.set(Some(timer));
    }

    fn set_element_fullscreen(&self, active: bool) {
        if self.element_fullscreen.get() == active {
            return;
        }
        self.element_fullscreen.set(active);
        self.fullscreen_view
            .set(if active { self.current_view() } else { None });
        self.header.set_visible(!active);
        self.toolbar.set_visible(!active);
        self.render_status();
        let window = self.window();
        if active {
            if let Some(window) = &window {
                window.fullscreen();
            }
            self.show_fullscreen_notice(self.fullscreen_view.get());
        } else {
            self.hide_fullscreen_notice();
            if let Some(window) = &window {
                window.unfullscreen();
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

    fn update_active_tab(&self) {
        let current = self.notebook.current();
        let closable = self.notebook.n_pages() > 1;
        for tab in self.tabs.borrow().iter() {
            tab.wrapper
                .set_css_class(c"ns-tab-active", Some(&tab.page) == current.as_ref());
            tab.close.set_visible(closable);
        }
    }

    fn tab_parts(&self, page: &Widget) -> Option<(gtk::Label, gtk::Image, gtk::Spinner)> {
        let tabs = self.tabs.borrow();
        let tab = tabs.iter().find(|tab| &tab.page == page)?;
        Some((tab.label.clone(), tab.icon.clone(), tab.spinner.clone()))
    }

    fn close_page(&self, page: &Widget) {
        let tab = {
            let mut tabs = self.tabs.borrow_mut();
            let at = tabs.iter().position(|tab| &tab.page == page);
            at.map(|at| tabs.remove(at))
        };
        let idx = self.notebook.page_num(page);
        if self.element_fullscreen.get() && view::for_page(page) == self.fullscreen_view.get() {
            self.set_element_fullscreen(false);
        }
        if let Some(idx) = idx {
            self.notebook.remove_page(idx);
        }
        if let Some(tab) = tab {
            self.tabstrip.remove(&tab.wrapper);
        }
        if self.notebook.n_pages() == 0 {
            if let Some(window) = self.window() {
                window.close();
            }
        } else {
            self.update_active_tab();
        }
    }

    fn tab_button(&self, page: &Widget, private_mode: bool) -> (gtk::Button, Tab) {
        let tabbtn = gtk::Button::new();
        tabbtn.set_has_frame(false);
        tabbtn.add_css_class(c"ns-tab-label");
        let tabcontent = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 5);
        let icon = icons::image_new(if private_mode {
            c"southstar-private-symbolic"
        } else {
            c"southstar-page-symbolic"
        });
        icon.set_pixel_size(16);
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(16, 16);
        spinner.set_visible(false);
        if private_mode {
            tabbtn.set_tooltip_text(Some(tr(c"Private tab")));
        }
        let label = gtk::Label::new(if private_mode {
            tr(c"Private tab")
        } else {
            tr(c"New Tab")
        });
        label.set_ellipsize(gtk_sys::ELLIPSIZE_END);
        label.set_width_chars(10);
        label.set_max_width_chars(22);
        label.set_xalign(0.0);
        tabcontent.append(&spinner);
        tabcontent.append(&icon);
        tabcontent.append(&label);
        tabbtn.set_child(&tabcontent);
        let weak_page = page.downgrade();
        tabbtn.connect_clicked(on(&self.me, move |pw| {
            if let Some(page) = weak_page.upgrade()
                && let Some(idx) = pw.notebook.page_num(&page)
            {
                pw.notebook.set_current_page(idx);
            }
        }));

        let close = icons::button_new(c"southstar-close-symbolic");
        close.set_has_frame(false);
        close.add_css_class(c"ns-tab-close");
        close.set_valign(gtk_sys::ALIGN_CENTER);
        close.set_tooltip_text(Some(tr(c"Close tab")));
        close.set_accessible_label(tr(c"Close tab"));
        let weak_page = page.downgrade();
        close.connect_clicked(on(&self.me, move |pw| {
            if let Some(page) = weak_page.upgrade() {
                pw.close_page(&page);
            }
        }));

        let wrapper = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 0);
        wrapper.add_css_class(c"ns-tab");
        if private_mode {
            wrapper.add_css_class(c"ns-tab-private");
        }
        let tab = Tab {
            page: page.clone(),
            wrapper,
            close,
            label,
            icon,
            spinner,
        };
        (tabbtn, tab)
    }

    pub fn add_tab_full(&self, url: &CStr, foreground: bool, private_mode: bool) {
        if !private_mode
            && southstar_renderer_host::single_process_enabled()
            && self.notebook.n_pages() > 0
            && let Some(cur) = self.current_view()
        {
            let r = normalize_url(url);
            self.address.set_text(&r);
            cur.load(&r);
            return;
        }

        let Some(v) = View::create() else {
            return;
        };
        if private_mode {
            v.set_private(true);
        }
        // SAFETY: on_view_notify reads only the view and text it is given, and ignores the null data.
        unsafe { v.set_notify(on_view_notify, ptr::null_mut()) };
        let Some(page) = v.attach_to_page() else {
            return;
        };
        page.object().set_data(PW_KEY, Some(self.me.clone()));

        let (tabbtn, tab) = self.tab_button(&page, private_mode);
        let wrapper = tab.wrapper.clone();
        wrapper.append(&tabbtn);
        wrapper.append(&tab.close);
        self.tabs.borrow_mut().push(tab);

        let idx = self.notebook.append_page(&page, &gtk::Label::new(c""));

        self.tabstrip.remove(&self.newtab_btn);
        self.tabstrip.append(&wrapper);
        self.tabstrip.append(&self.newtab_btn);

        if foreground {
            self.notebook.set_current_page(idx);
        }
        self.update_active_tab();

        v.load(&normalize_url(url));
    }

    pub fn add_tab(&self, url: &CStr, foreground: bool) {
        self.add_tab_full(url, foreground, false);
    }

    fn activate_address(&self) {
        let resolved = normalize_url(&self.address.text());
        if resolved.is_empty() {
            return;
        }
        let v = match self.current_view() {
            None => {
                self.add_tab(&resolved, true);
                self.current_view()
            }
            Some(v) => {
                self.address.set_text(&resolved);
                v.load(&resolved);
                Some(v)
            }
        };
        if let Some(v) = v {
            v.focus();
        }
    }

    fn address_key_pressed(&self, keyval: c_uint, state: gtk_sys::GdkModifierType) -> bool {
        if keyval != gtk_sys::KEY_RETURN && keyval != gtk_sys::KEY_KP_ENTER {
            return false;
        }
        let ctrl = state & gtk_sys::CONTROL_MASK != 0;
        let shift = state & gtk_sys::SHIFT_MASK != 0;
        if !ctrl && !shift {
            return false;
        }
        let Some(host) = rules::completed_host(self.address.text().to_bytes(), ctrl, shift) else {
            return false;
        };
        self.address.set_text(&cstring(host));
        self.activate_address();
        true
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
        let Some(win) = self.window() else {
            return;
        };
        if win.is_fullscreen() {
            win.unfullscreen();
            win.maximize();
        } else {
            win.fullscreen();
        }
    }

    fn window_key_pressed(&self, keyval: c_uint) -> bool {
        if self.element_fullscreen.get()
            && (keyval == gtk_sys::KEY_F11 || keyval == gtk_sys::KEY_ESCAPE)
        {
            self.leave_element_fullscreen();
            return true;
        }
        if keyval == gtk_sys::KEY_F11 {
            self.toggle_window_fullscreen();
            return true;
        }
        if keyval == gtk_sys::KEY_ESCAPE
            && let Some(win) = self.window().filter(gtk::Window::is_fullscreen)
        {
            win.unfullscreen();
            win.maximize();
            return true;
        }
        false
    }

    fn load_in_current(&self, url: &CStr) {
        if let Some(v) = self.current_view() {
            v.load(url);
        }
    }

    fn on_switch_page(&self, page: &Widget) {
        if self.element_fullscreen.get() && view::for_page(page) != self.fullscreen_view.get() {
            self.leave_element_fullscreen();
        }
        self.update_active_tab();
        self.update_chrome();
    }
}

pub fn normalize_url(input: &CStr) -> CString {
    let trimmed = rules::strip(input.to_bytes());
    if trimmed.is_empty() || rules::is_explicit_url(trimmed) {
        return cstring(trimmed);
    }
    let trimmed = cstring(trimmed);
    let trimmed = trimmed.to_bytes();
    if let Some(local) = southstar_netutil::url_from_local_path(trimmed) {
        return cstring(local);
    }
    if southstar_netutil::address_is_search(trimmed) {
        return cstring(southstar_netutil::search_url_for(trimmed));
    }
    cstring(rules::https_url(trimmed))
}

fn address_display_url(url: Option<&CStr>) -> CString {
    let Some(url) = url.filter(|u| !u.is_empty()) else {
        return CString::default();
    };
    if !url.to_bytes().contains(&b'%') {
        return url.to_owned();
    }
    let Some(dec) = rules::uri_unescape(url.to_bytes()) else {
        return url.to_owned();
    };
    match String::from_utf8(dec) {
        Ok(text) if rules::display_is_safe(&text) => cstring(text),
        _ => url.to_owned(),
    }
}

fn set_tab_loading(icon: &gtk::Image, spinner: &gtk::Spinner, loading: bool) {
    spinner.set_spinning(loading);
    spinner.set_visible(loading);
    icon.set_visible(!loading);
}

/// # Safety
/// `raw` is null or a live view, and `text` null or a NUL-terminated string valid for the call.
unsafe extern "C" fn on_view_notify(
    raw: *mut view::NsProcView,
    evt: c_int,
    text: *const c_char,
    _data: *mut c_void,
) {
    // SAFETY: the caller passes null or the live view raising the event.
    let Some(v) = (unsafe { View::from_raw(raw.cast()) }) else {
        return;
    };
    let Some(page) = v.page() else {
        return;
    };
    let Some(pw) = page
        .object()
        .data::<Weak<ProcWindow>>(PW_KEY)
        .and_then(|me| me.upgrade())
    else {
        return;
    };
    // SAFETY: the caller passes null or a NUL-terminated string valid for the call.
    let text = unsafe { borrowed(text) };
    view_event(&pw, v, &page, evt, text);
}

fn view_event(pw: &ProcWindow, v: View, page: &Widget, evt: c_int, text: Option<&CStr>) {
    let in_notebook = pw.notebook.page_num(page).is_some();
    let is_current = Some(v) == pw.current_view();
    let parts = if in_notebook {
        pw.tab_parts(page)
    } else {
        None
    };
    match evt {
        EVT_TITLE => {
            if !v.is_private() {
                let url = v.url();
                southstar_history::record(url.as_deref(), text);
            }
            if let Some((label, _, _)) = &parts {
                let t = text.filter(|t| !t.is_empty()).unwrap_or(tr(c"Untitled"));
                label.set_text(&cstring(rules::clip_title(t.to_bytes())));
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
                pw.update_navigation(v);
            }
        }
        EVT_NEWTAB => {
            if let Some(t) = text.filter(|t| !t.is_empty()) {
                pw.add_tab_full(t, false, v.is_private());
            }
        }
        EVT_LOADING => {
            let loading = text.is_some_and(|t| t.to_bytes().first() == Some(&b'1'));
            if let Some((_, icon, spinner)) = &parts {
                set_tab_loading(icon, spinner, loading);
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
                if !crate::procview::nav::is_local_url(target) || v.is_local_page() {
                    let target = cstring(target);
                    let name = name.filter(|n| !n.is_empty()).map(cstring);
                    let top = v.url();
                    if let Some(window) = pw.window_widget() {
                        downloads::start(&window, &target, name.as_deref(), top.as_deref());
                    }
                }
            }
        }
        EVT_FAVICON if !v.is_private() => {
            if let Some((_, icon, _)) = &parts {
                match v.favicon() {
                    Some(fav) => icon.set_from_texture(&fav),
                    None => icons::image_set(icon, c"southstar-page-symbolic"),
                }
                icon.set_pixel_size(16);
            }
        }
        _ => {}
    }
}

fn load_home_page(v: View) {
    let home = {
        let config = southstar_config::lock();
        config.reload();
        let snapshot = config.get();
        let home = snapshot
            .home_url
            .c_str()
            .filter(|h| !h.is_empty())
            .unwrap_or(c"about:start");
        normalize_url(home)
    };
    v.load(&home);
}

macro_rules! view_action {
    ($name:ident, |$v:ident| $body:expr) => {
        fn $name(pw: &ProcWindow) {
            if let Some($v) = pw.current_view() {
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
view_action!(act_hard_reload, |v| {
    southstar_http_cache::clear();
    v.reload()
});

fn act_reload_or_stop(pw: &ProcWindow) {
    let Some(v) = pw.current_view() else {
        return;
    };
    if pw.loading.get() {
        v.stop();
    } else {
        v.reload();
    }
}

fn act_bookmark_page(pw: &ProcWindow) {
    pw.toggle_bookmark();
}

fn act_view_source(pw: &ProcWindow) {
    let Some(v) = pw.current_view() else {
        return;
    };
    let Some(url) = v.url() else {
        return;
    };
    if url.is_empty() || url.to_bytes().starts_with(b"view-source:") {
        return;
    }
    v.load(&cstring(
        [b"view-source:".as_slice(), url.to_bytes()].concat(),
    ));
}

fn act_new_tab(pw: &ProcWindow) {
    pw.add_tab(c"about:start", true);
}

fn act_new_private_tab(pw: &ProcWindow) {
    pw.add_tab_full(c"about:start", true, true);
}

fn act_close_tab(pw: &ProcWindow) {
    if let Some(page) = pw.notebook.current() {
        pw.close_page(&page);
    }
}

fn act_focus_address(pw: &ProcWindow) {
    pw.address.grab_focus();
    pw.address.select_region(0, -1);
}

fn act_focus_page(pw: &ProcWindow) {
    let v = pw.current_view();
    let address: &Widget = &pw.address;
    let editing_address = pw
        .window()
        .and_then(|w| w.focus())
        .is_some_and(|focus| &focus == address || focus.is_ancestor(address));
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
    let n = pw.notebook.n_pages();
    if n <= 1 {
        return;
    }
    let idx = pw.notebook.current_page().unwrap_or(-1);
    pw.notebook.set_current_page(rules::step_tab(idx, delta, n));
}

fn act_next_tab(pw: &ProcWindow) {
    step_tab(pw, 1);
}

fn act_prev_tab(pw: &ProcWindow) {
    step_tab(pw, -1);
}

fn act_quit(pw: &ProcWindow) {
    if let Some(window) = pw.window() {
        window.close();
    }
}

fn act_downloads(pw: &ProcWindow) {
    if let Some(window) = pw.window_widget() {
        downloads::show(&window);
    }
}

fn act_task_manager(pw: &ProcWindow) {
    if let Some(window) = pw.window_widget() {
        taskmgr::show(&window, &pw.notebook);
    }
}

fn act_print(pw: &ProcWindow) {
    view::print(pw.current_view());
}

fn act_save_pdf(pw: &ProcWindow) {
    view::save_pdf(pw.current_view());
}

fn act_save_image(pw: &ProcWindow) {
    view::save_image(pw.current_view());
}

fn act_fullscreen(pw: &ProcWindow) {
    pw.toggle_window_fullscreen();
}

fn act_project_page(pw: &ProcWindow) {
    pw.load_in_current(rules::PROJECT_URL);
}

fn act_reset_zoom_indicator(pw: &ProcWindow) {
    act_zoom_reset(pw);
}

fn action_for(name: &CStr) -> fn(&ProcWindow) {
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

fn install_shortcuts(pw: &ProcWindow, app: &gtk::Application, window: &gtk::ApplicationWindow) {
    let single_process = southstar_renderer_host::single_process_enabled();
    for shortcut in rules::SHORTCUTS {
        let action = gtk::SimpleAction::new(shortcut.name);
        action.connect_activate(on(&pw.me, action_for(shortcut.name)));
        window.add_action(&action);
        if !shortcut.accels.is_empty() {
            let full = cstring([b"win.".as_slice(), shortcut.name.to_bytes()].concat());
            app.set_accels_for_action(&full, shortcut.accels);
        }
        if single_process && matches!(shortcut.name.to_bytes(), b"new-tab" | b"new-private-tab") {
            action.set_enabled(false);
        }
    }
}

fn build_menu() -> gtk::Menu {
    let appmenu = gtk::Menu::new();
    for section in rules::MENU {
        let sec = gtk::Menu::new();
        for item in *section {
            let mi = gtk::MenuItem::new(tr(item.label), item.action);
            if let Some(accel) = item.accel {
                mi.set_attribute_string(c"accel", accel);
            }
            sec.append_item(&mi);
        }
        appmenu.append_section(&sec);
    }
    let about = gtk::Menu::new();
    about.append(tr(c"About Southstar"), c"win.about");
    appmenu.append_section(&about);
    appmenu
}

fn popdown_from(button: &gtk::WeakWidget) {
    if let Some(pop) = button
        .upgrade()
        .and_then(|b| b.ancestor(gtk::Popover::static_type()))
        .as_ref()
        .and_then(gtk::Popover::cast)
    {
        pop.popdown();
    }
}

impl ProcWindow {
    fn bookmark_row(&self, title: &CStr, url: &CStr) -> gtk::Box {
        let row = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 4);
        let open = gtk::Button::with_label(title);
        open.set_has_frame(false);
        open.set_hexpand(true);
        open.set_halign(gtk_sys::ALIGN_START);
        open.set_tooltip_text(Some(url));
        let weak_open = open.downgrade();
        let target = url.to_owned();
        open.connect_clicked(on(&self.me, move |pw| {
            if let Some(v) = pw.current_view() {
                v.load(&target);
            }
            popdown_from(&weak_open);
        }));
        let del = gtk::Button::from_icon_name(c"user-trash-symbolic");
        del.set_has_frame(false);
        del.set_tooltip_text(Some(tr(c"Remove bookmark")));
        del.set_accessible_label(tr(c"Remove bookmark"));
        let weak_row = row.downgrade();
        let target = url.to_owned();
        del.connect_clicked(on(&self.me, move |pw| {
            pw.bookmarks.borrow_mut().remove(&target);
            if let Some(row) = weak_row.upgrade()
                && let Some(list) = row.parent().as_ref().and_then(gtk::Box::cast)
            {
                list.remove(&row);
            }
            pw.update_bookmark_indicator();
        }));
        row.append(&open);
        row.append(&del);
        row
    }

    fn build_bookmarks_popover(&self) -> gtk::Popover {
        let vbox = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 4);
        vbox.set_margins(6, 6, 6, 6);
        vbox.set_size_request(320, -1);

        let add = gtk::Button::with_label(if self.current_page_bookmarked() {
            tr(c"Remove this bookmark")
        } else {
            tr(c"Bookmark this page")
        });
        let weak_add = add.downgrade();
        add.connect_clicked(on(&self.me, move |pw| {
            if pw.toggle_bookmark() {
                popdown_from(&weak_add);
            }
        }));
        vbox.append(&add);
        vbox.append(&gtk::Separator::new(gtk::ORIENTATION_HORIZONTAL));

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk_sys::POLICY_NEVER, gtk_sys::POLICY_AUTOMATIC);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(320);
        let list = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 2);
        let saved: Vec<(CString, CString)> = self
            .bookmarks
            .borrow()
            .items()
            .iter()
            .map(|b| (b.url().to_owned(), b.title().to_owned()))
            .collect();
        if saved.is_empty() {
            let empty = gtk::Label::new(tr(c"No bookmarks yet"));
            empty.add_css_class(c"dim-label");
            empty.set_margins(0, 0, 12, 12);
            list.append(&empty);
        }
        for (url, title) in &saved {
            let title = Some(title.as_c_str())
                .filter(|t| !t.is_empty())
                .unwrap_or(url);
            list.append(&self.bookmark_row(title, url));
        }
        scroll.set_child(&list);
        vbox.append(&scroll);

        let pop = gtk::Popover::new();
        pop.set_child(&vbox);
        pop
    }

    fn show_bookmarks(&self, button: &Widget) {
        let pop = self.build_bookmarks_popover();
        pop.set_parent(button);
        pop.set_position(gtk_sys::POS_BOTTOM);
        let weak_pop = pop.downgrade();
        pop.connect_closed(move || {
            if let Some(pop) = weak_pop.upgrade() {
                glib::idle_add_local_once(move || {
                    if pop.parent().is_some() {
                        pop.unparent();
                    }
                });
            }
        });
        pop.popup();
    }
}

fn build_address(me: &Weak<ProcWindow>) -> gtk::Entry {
    let address = gtk::Entry::new();
    address.set_hexpand(true);
    address.set_valign(gtk_sys::ALIGN_CENTER);
    address.add_css_class(c"ns-address");
    icons::entry_set(
        &address,
        gtk_sys::ENTRY_ICON_PRIMARY,
        c"southstar-globe-symbolic",
    );
    address.set_icon_tooltip_text(gtk_sys::ENTRY_ICON_PRIMARY, tr(c"Page location"));
    address.set_placeholder_text(tr(c"Search or enter a URL"));
    address.set_accessible_label(tr(c"Address and search bar"));
    address.connect_activate(on(me, ProcWindow::activate_address));
    let icon_me = me.clone();
    address.connect_icon_press(move |pos| {
        if pos == gtk_sys::ENTRY_ICON_SECONDARY
            && let Some(pw) = icon_me.upgrade()
        {
            act_bookmark_page(&pw);
        }
    });

    let focus = gtk::EventControllerFocus::new();
    focus.connect_enter(on(me, |pw| {
        let address = pw.address.clone();
        glib::idle_add_local_once(move || address.select_region(0, -1));
    }));
    address.add_controller(focus);

    let click = gtk::GestureClick::new(1);
    click.set_propagation_phase(gtk_sys::PHASE_CAPTURE);
    let pressed_me = me.clone();
    click.connect_pressed(move |_, n_press, _, _| {
        if let Some(pw) = pressed_me.upgrade() {
            let focused = pw.address.has_focus_within();
            pw.address_click_focuses.set(n_press == 1 && !focused);
        }
    });
    let released_me = me.clone();
    click.connect_released(move |_, _, _, _| {
        let Some(pw) = released_me.upgrade() else {
            return;
        };
        if !pw.address_click_focuses.replace(false) {
            return;
        }
        let address = pw.address.clone();
        glib::idle_add_local_once(move || {
            if !address.has_selection() {
                address.select_region(0, -1);
            }
        });
    });
    address.add_controller(click);

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk_sys::PHASE_CAPTURE);
    let keys_me = me.clone();
    keys.connect_key_pressed(move |keyval, _, state| {
        keys_me
            .upgrade()
            .is_some_and(|pw| pw.address_key_pressed(keyval, state))
    });
    address.add_controller(keys);
    address
}

fn page_label(class: &CStr, halign: c_int, valign: c_int) -> gtk::Label {
    let l = gtk::Label::new(c"");
    l.set_ellipsize(gtk_sys::ELLIPSIZE_MIDDLE);
    l.set_max_width_chars(90);
    l.add_css_class(class);
    l.set_halign(halign);
    l.set_valign(valign);
    l.set_can_target(false);
    l.set_visible(false);
    l
}

fn build_window(
    me: &Weak<ProcWindow>,
    app_window: &gtk::ApplicationWindow,
    initial_size: Option<(c_int, c_int)>,
) -> ProcWindow {
    let window = app_window.window();
    window.set_title(brand_versioned());
    match initial_size {
        Some((w, h)) => window.set_default_size(w, h),
        None => {
            window.set_default_size(1024, 768);
            window.maximize();
        }
    }

    let winkeys = gtk::EventControllerKey::new();
    winkeys.set_propagation_phase(gtk_sys::PHASE_CAPTURE);
    let keys_me = me.clone();
    winkeys.connect_key_pressed(move |keyval, _, _| {
        keys_me
            .upgrade()
            .is_some_and(|pw| pw.window_key_pressed(keyval))
    });
    window.add_controller(winkeys);

    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);
    let tabstrip = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 2);
    tabstrip.add_css_class(c"ns-tabstrip");
    let newtab = icons::button_new(c"southstar-new-tab-symbolic");
    newtab.set_has_frame(false);
    newtab.add_css_class(c"ns-newtab");
    newtab.set_tooltip_text(Some(tr(c"New tab")));
    newtab.set_accessible_label(tr(c"New tab"));
    newtab.connect_clicked(on(me, act_new_tab));
    newtab.set_valign(gtk_sys::ALIGN_CENTER);
    tabstrip.append(&newtab);
    if southstar_renderer_host::single_process_enabled() {
        newtab.set_visible(false);
    }
    header.set_title_widget(&ns_title_row_new(&tabstrip));
    window.set_titlebar(&header);

    let vbox = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 0);
    let toolbar = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 2);
    toolbar.add_css_class(c"ns-toolbar");

    let back = toolbar_button(
        me,
        c"southstar-back-symbolic",
        tr(c"Go back one page"),
        act_back,
    );
    let forward = toolbar_button(
        me,
        c"southstar-forward-symbolic",
        tr(c"Go forward one page"),
        act_forward,
    );
    let reload = toolbar_button(
        me,
        c"southstar-reload-symbolic",
        tr(c"Reload this page"),
        act_reload_or_stop,
    );
    let home = toolbar_button(
        me,
        c"southstar-home-symbolic",
        tr(c"Go to the home page"),
        act_home,
    );
    let downloads_btn = toolbar_button(
        me,
        c"southstar-downloads-symbolic",
        tr(c"Show downloads"),
        act_downloads,
    );

    let address = build_address(me);

    let zoom = gtk::Button::with_label(c"100%");
    zoom.set_has_frame(false);
    zoom.add_css_class(c"ns-zoom");
    zoom.set_valign(gtk_sys::ALIGN_CENTER);
    zoom.set_tooltip_text(Some(tr(c"Reset zoom (Ctrl+0)")));
    zoom.set_accessible_label(tr(c"Reset zoom"));
    zoom.set_visible(false);
    zoom.connect_clicked(on(me, act_reset_zoom_indicator));

    let bookmarks_button = nav_button(c"southstar-bookmarks-symbolic", tr(c"Bookmarks"));
    let weak_bookmarks = bookmarks_button.downgrade();
    bookmarks_button.connect_clicked(on(me, move |pw| {
        if let Some(button) = weak_bookmarks.upgrade() {
            pw.show_bookmarks(&button);
        }
    }));

    let menu_button = gtk::MenuButton::new();
    icons::menu_button_set(&menu_button, c"southstar-menu-symbolic");
    menu_button.add_css_class(c"ns-nav-button");
    menu_button.set_valign(gtk_sys::ALIGN_CENTER);
    menu_button.set_menu_model(&build_menu());
    ns_popover_menu_fit(
        menu_button
            .popover()
            .map_or(ptr::null_mut(), |pop| pop.as_ptr()),
    );
    menu_button.set_tooltip_text(Some(tr(c"Menu")));
    menu_button.set_accessible_label(tr(c"Menu"));

    let logo = toolbar_button(
        me,
        c"southstar",
        tr(c"Visit the project page"),
        act_project_page,
    );
    logo.add_css_class(c"ns-logo");

    for w in [
        &*back,
        &*forward,
        &*reload,
        &*home,
        &*address,
        &*zoom,
        &*bookmarks_button,
        &*downloads_btn,
        &*menu_button,
        &*logo,
    ] {
        toolbar.append(w);
    }
    vbox.append(&toolbar);

    let notebook = gtk::Notebook::new();
    notebook.set_show_tabs(false);
    notebook.set_show_border(false);
    notebook.set_scrollable(true);
    notebook.set_hexpand(true);
    notebook.set_vexpand(true);
    let switch_me = me.clone();
    notebook.connect_switch_page_after(move |page, _| {
        if let Some(pw) = switch_me.upgrade() {
            pw.on_switch_page(page);
        }
    });

    let status_label = page_label(c"ns-procstatus", gtk_sys::ALIGN_START, gtk_sys::ALIGN_END);
    let fullscreen_notice = page_label(
        c"ns-fullscreen-notice",
        gtk_sys::ALIGN_CENTER,
        gtk_sys::ALIGN_START,
    );

    let overlay = gtk::Overlay::new();
    overlay.set_child(&notebook);
    overlay.add_overlay(&status_label);
    overlay.add_overlay(&fullscreen_notice);
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    vbox.append(&overlay);

    window.set_child(&vbox);

    ProcWindow {
        me: me.clone(),
        window: window.downgrade(),
        header,
        toolbar,
        notebook,
        tabstrip,
        newtab_btn: newtab,
        tabs: RefCell::new(Vec::new()),
        address,
        address_click_focuses: Cell::new(false),
        zoom_button: zoom,
        back,
        forward,
        reload,
        loading: Cell::new(false),
        status_label,
        status_base: RefCell::new(CString::default()),
        status_timer: Cell::new(None),
        theme_watch: RefCell::new(None),
        webgl_active: Cell::new(false),
        element_fullscreen: Cell::new(false),
        fullscreen_view: Cell::new(None),
        fullscreen_notice,
        fullscreen_notice_timer: Cell::new(None),
        bookmarks: RefCell::new(Bookmarks::load()),
        session_path: RefCell::new(None),
        session_timer: Cell::new(None),
    }
}

pub fn proc_window_new(
    app: &gtk::Application,
    initial_size: Option<(c_int, c_int)>,
) -> Rc<ProcWindow> {
    let app_window = gtk::ApplicationWindow::new(app);
    let pw = Rc::new_cyclic(|me| build_window(me, &app_window, initial_size));
    app_window
        .object()
        .set_data(WINDOW_KEY, Some(Rc::clone(&pw)));
    install_shortcuts(&pw, app, &app_window);

    if let Some(settings) = gtk::default_settings() {
        let ids = [
            settings.connect_notify(
                c"gtk-theme-name",
                on(&pw.me, ProcWindow::apply_color_scheme),
            ),
            settings.connect_notify(
                c"gtk-application-prefer-dark-theme",
                on(&pw.me, ProcWindow::apply_color_scheme),
            ),
        ];
        *pw.theme_watch.borrow_mut() = Some((settings, ids));
    }
    pw
}

impl ProcWindow {
    fn write_session(&self) -> bool {
        let path = self.session_path.borrow().clone();
        let Some(path) = path else {
            return false;
        };
        let mut s: Vec<u8> = Vec::new();
        for v in self.views() {
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
        if let Some(path) = southstar_glib::path_from_bytes(path.to_bytes()) {
            let mut tmp = path.clone().into_os_string();
            tmp.push(".tmp");
            if std::fs::write(&tmp, &s).is_ok() && std::fs::rename(&tmp, &path).is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
        }
        true
    }

    pub fn start_session_timer(&self) {
        if self.session_path.borrow().is_none() {
            return;
        }
        let me = self.me.clone();
        let timer = glib::timeout_add_seconds_local(4, move || {
            let Some(pw) = me.upgrade() else {
                return ControlFlow::Break(());
            };
            if pw.write_session() {
                ControlFlow::Continue(())
            } else {
                pw.session_timer.set(None);
                ControlFlow::Break(())
            }
        });
        if let Some(old) = self.session_timer.replace(Some(timer)) {
            old.remove();
        }
    }

    pub fn present(&self) {
        if let Some(window) = self.window() {
            window.present();
        }
    }

    pub fn apply_scheme(&self) {
        self.apply_color_scheme();
    }

    pub fn status(&self, text: &CStr) {
        self.set_status(text);
    }
}
