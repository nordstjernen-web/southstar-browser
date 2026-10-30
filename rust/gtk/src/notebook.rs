//! Southstar — the containers a browser window is built from: notebook, overlay, header bar, spinner and separator, plus the toplevel's window-state calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint, c_void};

use southstar_gtk_ffi::{self as sys, GtkWidget};

use crate::event::{borrow_object, connect_boxed_after, gcallback};
use crate::signal::SignalHandlerId;
use crate::{HeaderBar, Widget, Window, ffi};

crate::widgets::widget_class!(Notebook => ffi::gtk_notebook_get_type);
crate::widgets::widget_type!(Overlay, Spinner, Separator);

unsafe extern "C" fn on_switch_page<F: Fn(&Widget, c_uint) + 'static>(
    _notebook: *mut c_void,
    page: *mut GtkWidget,
    page_num: c_uint,
    data: *mut c_void,
) {
    // SAFETY: GTK passes the live page widget being switched to.
    let page = Widget(unsafe { borrow_object(page.cast()) });
    // SAFETY: data is the boxed F connect_boxed_after registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(&page, page_num);
}

impl Notebook {
    pub fn new() -> Notebook {
        // SAFETY: gtk_notebook_new returns a new floating widget.
        Notebook(Widget::adopt(unsafe { sys::gtk_notebook_new() }))
    }

    pub fn set_show_tabs(&self, show: bool) {
        // SAFETY: self is a live GtkNotebook.
        unsafe { sys::gtk_notebook_set_show_tabs(self.as_ptr(), show.into()) };
    }

    pub fn set_show_border(&self, show: bool) {
        // SAFETY: self is a live GtkNotebook.
        unsafe { sys::gtk_notebook_set_show_border(self.as_ptr(), show.into()) };
    }

    pub fn set_scrollable(&self, scrollable: bool) {
        // SAFETY: self is a live GtkNotebook.
        unsafe { sys::gtk_notebook_set_scrollable(self.as_ptr(), scrollable.into()) };
    }

    pub fn n_pages(&self) -> c_int {
        // SAFETY: self is a live GtkNotebook.
        unsafe { sys::gtk_notebook_get_n_pages(self.as_ptr()) }
    }

    pub fn current_page(&self) -> Option<c_int> {
        // SAFETY: self is a live GtkNotebook.
        let page = unsafe { sys::gtk_notebook_get_current_page(self.as_ptr()) };
        (page >= 0).then_some(page)
    }

    pub fn set_current_page(&self, page_num: c_int) {
        // SAFETY: self is a live GtkNotebook; GTK ignores an out-of-range index and takes -1 as the last page.
        unsafe { sys::gtk_notebook_set_current_page(self.as_ptr(), page_num) };
    }

    pub fn nth_page(&self, page_num: c_int) -> Option<Widget> {
        // SAFETY: self is a live GtkNotebook; an out-of-range index returns null.
        let page = unsafe { sys::gtk_notebook_get_nth_page(self.as_ptr(), page_num) };
        // SAFETY: GTK returned null or a live page the notebook holds.
        unsafe { Widget::from_ptr(page) }
    }

    pub fn current(&self) -> Option<Widget> {
        self.nth_page(self.current_page()?)
    }

    pub fn pages(&self) -> impl Iterator<Item = Widget> {
        (0..self.n_pages()).filter_map(|i| self.nth_page(i))
    }

    pub fn page_num(&self, child: &Widget) -> Option<c_int> {
        // SAFETY: self is a live GtkNotebook and child a live widget; GTK returns -1 for a widget that is not a page.
        let page = unsafe { sys::gtk_notebook_page_num(self.as_ptr(), child.as_ptr()) };
        (page >= 0).then_some(page)
    }

    pub fn append_page(&self, child: &impl AsRef<Widget>, tab_label: &impl AsRef<Widget>) -> c_int {
        // SAFETY: self is a live GtkNotebook and both widgets are live; the notebook takes its own references.
        unsafe {
            sys::gtk_notebook_append_page(
                self.as_ptr(),
                child.as_ref().as_ptr(),
                tab_label.as_ref().as_ptr(),
            )
        }
    }

    pub fn remove_page(&self, page_num: c_int) {
        // SAFETY: self is a live GtkNotebook; GTK checks the index.
        unsafe { sys::gtk_notebook_remove_page(self.as_ptr(), page_num) };
    }

    pub fn connect_switch_page_after<F: Fn(&Widget, c_uint) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        let callback = gcallback!(on_switch_page::<F> =>
            fn(*mut c_void, *mut GtkWidget, c_uint, *mut c_void));
        // SAFETY: GtkNotebook::switch-page takes (notebook, page, page_num, data) and returns nothing, as on_switch_page does.
        unsafe { connect_boxed_after(self.object(), c"switch-page", callback, f) }
    }
}

impl Default for Notebook {
    fn default() -> Notebook {
        Notebook::new()
    }
}

impl Overlay {
    pub fn new() -> Overlay {
        // SAFETY: gtk_overlay_new returns a new floating widget.
        Overlay(Widget::adopt(unsafe { sys::gtk_overlay_new() }))
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkOverlay and child a live widget; the overlay takes its own reference.
        unsafe { sys::gtk_overlay_set_child(self.as_ptr(), child.as_ref().as_ptr()) };
    }

    pub fn add_overlay(&self, widget: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkOverlay and widget a live widget; the overlay takes its own reference.
        unsafe { sys::gtk_overlay_add_overlay(self.as_ptr(), widget.as_ref().as_ptr()) };
    }
}

impl Default for Overlay {
    fn default() -> Overlay {
        Overlay::new()
    }
}

impl Spinner {
    pub fn new() -> Spinner {
        // SAFETY: gtk_spinner_new returns a new floating widget.
        Spinner(Widget::adopt(unsafe { sys::gtk_spinner_new() }))
    }

    pub fn set_spinning(&self, spinning: bool) {
        // SAFETY: self is a live GtkSpinner.
        unsafe { sys::gtk_spinner_set_spinning(self.as_ptr(), spinning.into()) };
    }
}

impl Default for Spinner {
    fn default() -> Spinner {
        Spinner::new()
    }
}

impl Separator {
    pub fn new(orientation: sys::GtkOrientation) -> Separator {
        // SAFETY: GTK checks the orientation and returns a new floating widget.
        Separator(Widget::adopt(unsafe {
            sys::gtk_separator_new(orientation)
        }))
    }
}

impl HeaderBar {
    pub fn new() -> HeaderBar {
        // SAFETY: gtk_header_bar_new returns a new floating widget.
        HeaderBar::cast(&Widget::adopt(unsafe { sys::gtk_header_bar_new() }))
            .expect("gtk_header_bar_new returns a GtkHeaderBar")
    }

    pub fn set_show_title_buttons(&self, show: bool) {
        // SAFETY: self is a live GtkHeaderBar.
        unsafe { sys::gtk_header_bar_set_show_title_buttons(self.as_ptr(), show.into()) };
    }

    pub fn set_title_widget(&self, widget: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkHeaderBar and widget a live widget; the bar takes its own reference.
        unsafe { sys::gtk_header_bar_set_title_widget(self.as_ptr(), widget.as_ref().as_ptr()) };
    }
}

impl Default for HeaderBar {
    fn default() -> HeaderBar {
        HeaderBar::new()
    }
}

impl Window {
    pub fn set_titlebar(&self, titlebar: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkWindow and titlebar a live widget; the window takes its own reference.
        unsafe { sys::gtk_window_set_titlebar(self.as_ptr(), titlebar.as_ref().as_ptr()) };
    }

    pub fn fullscreen(&self) {
        // SAFETY: self is a live GtkWindow.
        unsafe { sys::gtk_window_fullscreen(self.as_ptr()) };
    }

    pub fn unfullscreen(&self) {
        // SAFETY: self is a live GtkWindow.
        unsafe { sys::gtk_window_unfullscreen(self.as_ptr()) };
    }

    pub fn maximize(&self) {
        // SAFETY: self is a live GtkWindow.
        unsafe { sys::gtk_window_maximize(self.as_ptr()) };
    }

    pub fn is_fullscreen(&self) -> bool {
        // SAFETY: self is a live GtkWindow.
        unsafe { sys::gtk_window_is_fullscreen(self.as_ptr()) != 0 }
    }

    pub fn close(&self) {
        // SAFETY: self is a live GtkWindow; this handle's reference keeps the object valid if closing destroys it.
        unsafe { sys::gtk_window_close(self.as_ptr()) };
    }

    pub fn focus(&self) -> Option<Widget> {
        // SAFETY: self is a live GtkWindow.
        let focus = unsafe { sys::gtk_window_get_focus(self.as_ptr()) };
        // SAFETY: GTK returned null or the live focus widget inside the window.
        unsafe { Widget::from_ptr(focus) }
    }
}
