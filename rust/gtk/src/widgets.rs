//! Southstar — typed handles for the GTK widget classes the shell builds, each checked on downcast.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_gtk_ffi as sys;

use crate::signal::{self, SignalHandlerId};
use crate::{Widget, ffi};

macro_rules! widget_class {
    ($($name:ident => $get_type:path),* $(,)?) => {
        $(
            #[derive(Clone, PartialEq, Eq)]
            pub struct $name(Widget);

            impl core::ops::Deref for $name {
                type Target = Widget;

                fn deref(&self) -> &Widget {
                    &self.0
                }
            }

            impl From<$name> for Widget {
                fn from(widget: $name) -> Widget {
                    widget.0
                }
            }

            impl AsRef<Widget> for $name {
                fn as_ref(&self) -> &Widget {
                    &self.0
                }
            }

            impl $name {
                pub fn cast(widget: &Widget) -> Option<$name> {
                    // SAFETY: the type getter registers the class if needed and has no preconditions.
                    let type_ = unsafe { $get_type() };
                    widget.is_a(type_).then(|| $name(widget.clone()))
                }
            }
        )*
    };
}

widget_class!(
    Box => ffi::gtk_box_get_type,
    Label => ffi::gtk_label_get_type,
    Button => ffi::gtk_button_get_type,
    Image => sys::gtk_image_get_type,
    ListBox => ffi::gtk_list_box_get_type,
    ListBoxRow => ffi::gtk_list_box_row_get_type,
    ScrolledWindow => ffi::gtk_scrolled_window_get_type,
    TextView => ffi::gtk_text_view_get_type,
    Window => sys::gtk_window_get_type,
    ProgressBar => ffi::gtk_progress_bar_get_type,
);

impl Box {
    pub fn new(orientation: sys::GtkOrientation, spacing: c_int) -> Box {
        // SAFETY: gtk_box_new checks the orientation and returns a new floating widget.
        Box(Widget::adopt(unsafe {
            sys::gtk_box_new(orientation, spacing)
        }))
    }

    pub fn append(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkBox and child a live widget; the box takes its own reference.
        unsafe { sys::gtk_box_append(self.as_ptr(), child.as_ref().as_ptr()) };
    }
}

impl AsRef<Widget> for Widget {
    fn as_ref(&self) -> &Widget {
        self
    }
}

impl Label {
    pub fn new(text: &CStr) -> Label {
        // SAFETY: text is NUL-terminated; gtk_label_new returns a new floating widget.
        Label(Widget::adopt(unsafe { sys::gtk_label_new(text.as_ptr()) }))
    }

    pub fn set_text(&self, text: &CStr) {
        // SAFETY: self is a live GtkLabel and text is NUL-terminated.
        unsafe { sys::gtk_label_set_text(self.as_ptr(), text.as_ptr()) };
    }

    pub fn set_xalign(&self, xalign: f32) {
        // SAFETY: self is a live GtkLabel.
        unsafe { sys::gtk_label_set_xalign(self.as_ptr(), xalign) };
    }

    pub fn set_width_chars(&self, n_chars: c_int) {
        // SAFETY: self is a live GtkLabel.
        unsafe { sys::gtk_label_set_width_chars(self.as_ptr(), n_chars) };
    }

    pub fn set_ellipsize(&self, mode: c_int) {
        // SAFETY: self is a live GtkLabel; GTK checks the enum value.
        unsafe { sys::gtk_label_set_ellipsize(self.as_ptr(), mode) };
    }
}

impl Button {
    pub fn with_label(text: &CStr) -> Button {
        // SAFETY: text is NUL-terminated; the constructor returns a new floating widget.
        Button(Widget::adopt(unsafe {
            sys::gtk_button_new_with_label(text.as_ptr())
        }))
    }

    pub fn from_icon_name(icon: &CStr) -> Button {
        // SAFETY: icon is NUL-terminated; the constructor returns a new floating widget.
        Button(Widget::adopt(unsafe {
            sys::gtk_button_new_from_icon_name(icon.as_ptr())
        }))
    }

    pub fn set_has_frame(&self, frame: bool) {
        // SAFETY: self is a live GtkButton.
        unsafe { sys::gtk_button_set_has_frame(self.as_ptr(), frame.into()) };
    }

    pub fn connect_clicked(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"clicked", f)
    }
}

impl Image {
    pub fn from_icon_name(icon: &CStr) -> Image {
        // SAFETY: icon is NUL-terminated; the constructor returns a new floating widget.
        Image(Widget::adopt(unsafe {
            sys::gtk_image_new_from_icon_name(icon.as_ptr())
        }))
    }
}

impl ListBox {
    pub fn new() -> ListBox {
        // SAFETY: gtk_list_box_new returns a new floating widget.
        ListBox(Widget::adopt(unsafe { sys::gtk_list_box_new() }))
    }

    pub fn set_selection_mode(&self, mode: c_int) {
        // SAFETY: self is a live GtkListBox; GTK checks the enum value.
        unsafe { sys::gtk_list_box_set_selection_mode(self.as_ptr(), mode) };
    }

    pub fn append(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkListBox and child a live widget; the list takes its own reference.
        unsafe { sys::gtk_list_box_append(self.as_ptr(), child.as_ref().as_ptr()) };
    }

    pub fn prepend(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkListBox and child a live widget; the list takes its own reference.
        unsafe { sys::gtk_list_box_prepend(self.as_ptr(), child.as_ref().as_ptr()) };
    }

    pub fn remove(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkListBox; GTK checks that child is one of its rows.
        unsafe { sys::gtk_list_box_remove(self.as_ptr(), child.as_ref().as_ptr()) };
    }

    pub fn remove_all(&self) {
        while let Some(child) = self.first_child() {
            self.remove(&child);
        }
    }

    pub fn selected_row(&self) -> Option<ListBoxRow> {
        // SAFETY: self is a live GtkListBox.
        let row = unsafe { sys::gtk_list_box_get_selected_row(self.as_ptr()) };
        // SAFETY: GTK returned null or a live row the list holds.
        let row = unsafe { Widget::from_ptr(row) }?;
        ListBoxRow::cast(&row)
    }

    pub fn select_row(&self, row: &ListBoxRow) {
        // SAFETY: self is a live GtkListBox and row a live GtkListBoxRow.
        unsafe { sys::gtk_list_box_select_row(self.as_ptr(), row.as_ptr()) };
    }

    pub fn rows(&self) -> impl Iterator<Item = ListBoxRow> {
        self.children().filter_map(|child| ListBoxRow::cast(&child))
    }
}

impl Default for ListBox {
    fn default() -> ListBox {
        ListBox::new()
    }
}

impl ListBoxRow {
    pub fn new() -> ListBoxRow {
        // SAFETY: gtk_list_box_row_new returns a new floating widget.
        ListBoxRow(Widget::adopt(unsafe { sys::gtk_list_box_row_new() }))
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkListBoxRow and child a live widget; the row takes its own reference.
        unsafe { sys::gtk_list_box_row_set_child(self.as_ptr(), child.as_ref().as_ptr()) };
    }
}

impl Default for ListBoxRow {
    fn default() -> ListBoxRow {
        ListBoxRow::new()
    }
}

impl ScrolledWindow {
    pub fn new() -> ScrolledWindow {
        // SAFETY: gtk_scrolled_window_new returns a new floating widget.
        ScrolledWindow(Widget::adopt(unsafe { sys::gtk_scrolled_window_new() }))
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkScrolledWindow and child a live widget; it takes its own reference.
        unsafe { sys::gtk_scrolled_window_set_child(self.as_ptr(), child.as_ref().as_ptr()) };
    }
}

impl Default for ScrolledWindow {
    fn default() -> ScrolledWindow {
        ScrolledWindow::new()
    }
}

impl TextView {
    pub fn new() -> TextView {
        // SAFETY: gtk_text_view_new returns a new floating widget.
        TextView(Widget::adopt(unsafe { sys::gtk_text_view_new() }))
    }

    pub fn set_editable(&self, editable: bool) {
        // SAFETY: self is a live GtkTextView.
        unsafe { sys::gtk_text_view_set_editable(self.as_ptr(), editable.into()) };
    }

    pub fn set_cursor_visible(&self, visible: bool) {
        // SAFETY: self is a live GtkTextView.
        unsafe { sys::gtk_text_view_set_cursor_visible(self.as_ptr(), visible.into()) };
    }

    pub fn set_monospace(&self, monospace: bool) {
        // SAFETY: self is a live GtkTextView.
        unsafe { sys::gtk_text_view_set_monospace(self.as_ptr(), monospace.into()) };
    }

    pub fn set_wrap_mode(&self, mode: c_int) {
        // SAFETY: self is a live GtkTextView; GTK checks the enum value.
        unsafe { sys::gtk_text_view_set_wrap_mode(self.as_ptr(), mode) };
    }

    pub fn set_text(&self, text: &str) {
        let len = c_int::try_from(text.len()).unwrap_or(c_int::MAX);
        // SAFETY: self is a live GtkTextView; the buffer it returns is owned by the view.
        let buffer = unsafe { sys::gtk_text_view_get_buffer(self.as_ptr()) };
        // SAFETY: buffer is live and text is len bytes of valid UTF-8.
        unsafe { sys::gtk_text_buffer_set_text(buffer, text.as_ptr().cast(), len) };
    }
}

impl Default for TextView {
    fn default() -> TextView {
        TextView::new()
    }
}

impl Window {
    pub fn new() -> Window {
        // SAFETY: gtk_window_new returns a new toplevel that GTK keeps in its window list.
        Window(Widget::adopt(unsafe { sys::gtk_window_new() }))
    }

    pub fn set_title(&self, title: &CStr) {
        // SAFETY: self is a live GtkWindow and title is NUL-terminated.
        unsafe { sys::gtk_window_set_title(self.as_ptr(), title.as_ptr()) };
    }

    pub fn set_transient_for(&self, parent: Option<&Widget>) {
        let parent = parent.map_or(core::ptr::null_mut(), Widget::as_ptr);
        // SAFETY: self is a live GtkWindow; parent is null or a live widget GTK checks is a window.
        unsafe { sys::gtk_window_set_transient_for(self.as_ptr(), parent) };
    }

    pub fn set_default_size(&self, width: c_int, height: c_int) {
        // SAFETY: self is a live GtkWindow.
        unsafe { sys::gtk_window_set_default_size(self.as_ptr(), width, height) };
    }

    pub fn set_child(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkWindow and child a live widget; the window takes its own reference.
        unsafe { sys::gtk_window_set_child(self.as_ptr(), child.as_ref().as_ptr()) };
    }

    pub fn set_hide_on_close(&self, hide: bool) {
        // SAFETY: self is a live GtkWindow.
        unsafe { ffi::gtk_window_set_hide_on_close(self.as_ptr(), hide.into()) };
    }

    pub fn present(&self) {
        // SAFETY: self is a live GtkWindow.
        unsafe { sys::gtk_window_present(self.as_ptr()) };
    }

    pub fn destroy(&self) {
        // SAFETY: self is a live GtkWindow; this handle's reference keeps the object valid afterwards.
        unsafe { sys::gtk_window_destroy(self.as_ptr()) };
    }
}

impl Default for Window {
    fn default() -> Window {
        Window::new()
    }
}

impl ProgressBar {
    pub fn new() -> ProgressBar {
        // SAFETY: gtk_progress_bar_new returns a new floating widget.
        ProgressBar(Widget::adopt(unsafe { sys::gtk_progress_bar_new() }))
    }

    pub fn pulse(&self) {
        // SAFETY: self is a live GtkProgressBar.
        unsafe { sys::gtk_progress_bar_pulse(self.as_ptr()) };
    }

    pub fn set_fraction(&self, fraction: f64) {
        // SAFETY: self is a live GtkProgressBar; GTK clamps the fraction.
        unsafe { sys::gtk_progress_bar_set_fraction(self.as_ptr(), fraction) };
    }
}

impl Default for ProgressBar {
    fn default() -> ProgressBar {
        ProgressBar::new()
    }
}
