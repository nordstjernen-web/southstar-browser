//! Southstar — the text entry and its editable text, icon slots and signals, and the label, button and image setters a toolbar needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use std::ffi::CString;

use southstar_gtk_ffi::{self as sys, GdkRgba};

use crate::event::{connect_boxed, gcallback};
use crate::signal::{self, SignalHandlerId};
use crate::{Box, Button, Entry, Image, Label, Texture, Widget};

unsafe extern "C" fn on_icon_press<F: Fn(sys::GtkEntryIconPosition) + 'static>(
    _entry: *mut c_void,
    position: sys::GtkEntryIconPosition,
    data: *mut c_void,
) {
    // SAFETY: data is the boxed F connect_boxed registered, alive while the handler is.
    let f = unsafe { &*data.cast::<F>() };
    f(position);
}

impl Entry {
    pub fn new() -> Entry {
        // SAFETY: gtk_entry_new returns a new floating widget.
        Entry::cast(&Widget::adopt(unsafe { sys::gtk_entry_new() }))
            .expect("gtk_entry_new returns a GtkEntry")
    }

    pub fn text(&self) -> CString {
        // SAFETY: self is a live GtkEntry, a GtkEditable; the text is owned by the entry.
        let text = unsafe { sys::gtk_editable_get_text(self.as_ptr()) };
        if text.is_null() {
            return CString::default();
        }
        // SAFETY: text is a NUL-terminated string the entry keeps until its next change, copied here first.
        unsafe { CStr::from_ptr(text) }.to_owned()
    }

    pub fn set_text(&self, text: &CStr) {
        // SAFETY: self is a live GtkEditable and text is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_editable_set_text(self.as_ptr(), text.as_ptr()) };
    }

    pub fn select_region(&self, start: c_int, end: c_int) {
        // SAFETY: self is a live GtkEditable; GTK clamps the positions.
        unsafe { sys::gtk_editable_select_region(self.as_ptr(), start, end) };
    }

    pub fn has_selection(&self) -> bool {
        // SAFETY: self is a live GtkEditable; null out-pointers are allowed.
        unsafe {
            sys::gtk_editable_get_selection_bounds(
                self.as_ptr(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            ) != 0
        }
    }

    pub fn clear_icon(&self, pos: sys::GtkEntryIconPosition) {
        // SAFETY: self is a live GtkEntry; a null name removes the icon and GTK checks the position.
        unsafe {
            sys::gtk_entry_set_icon_from_icon_name(self.as_ptr().cast(), pos, core::ptr::null())
        };
    }

    pub fn set_icon_activatable(&self, pos: sys::GtkEntryIconPosition, activatable: bool) {
        // SAFETY: self is a live GtkEntry; GTK checks the position.
        unsafe { sys::gtk_entry_set_icon_activatable(self.as_ptr(), pos, activatable.into()) };
    }

    pub fn set_icon_tooltip_text(&self, pos: sys::GtkEntryIconPosition, tooltip: &CStr) {
        // SAFETY: self is a live GtkEntry, tooltip is NUL-terminated and GTK checks the position.
        unsafe { sys::gtk_entry_set_icon_tooltip_text(self.as_ptr(), pos, tooltip.as_ptr()) };
    }

    pub fn set_placeholder_text(&self, text: &CStr) {
        // SAFETY: self is a live GtkEntry and text is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_entry_set_placeholder_text(self.as_ptr(), text.as_ptr()) };
    }

    pub fn connect_activate(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"activate", f)
    }

    pub fn connect_icon_press<F: Fn(sys::GtkEntryIconPosition) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        let callback = gcallback!(on_icon_press::<F> =>
            fn(*mut c_void, sys::GtkEntryIconPosition, *mut c_void));
        // SAFETY: GtkEntry::icon-press takes (entry, icon_pos, data) and returns nothing, as on_icon_press does.
        unsafe { connect_boxed(self.object(), c"icon-press", callback, f) }
    }
}

impl Default for Entry {
    fn default() -> Entry {
        Entry::new()
    }
}

impl Widget {
    pub fn set_accessible_label(&self, label: &CStr) {
        // SAFETY: self is a live GtkAccessible; the property list is one label string followed by the -1 terminator.
        unsafe {
            sys::gtk_accessible_update_property(
                self.as_ptr(),
                sys::ACCESSIBLE_PROPERTY_LABEL,
                label.as_ptr(),
                -1 as c_int,
            )
        };
    }

    pub fn set_css_class(&self, class: &CStr, on: bool) {
        if on {
            self.add_css_class(class);
        } else {
            self.remove_css_class(class);
        }
    }

    pub fn has_focus_within(&self) -> bool {
        // SAFETY: self is a live widget.
        let flags = unsafe { sys::gtk_widget_get_state_flags(self.as_ptr()) };
        flags & sys::STATE_FLAG_FOCUS_WITHIN != 0
    }

    pub fn is_ancestor(&self, ancestor: &Widget) -> bool {
        // SAFETY: both widgets are live.
        unsafe { sys::gtk_widget_is_ancestor(self.as_ptr(), ancestor.as_ptr()) != 0 }
    }

    pub fn color(&self) -> GdkRgba {
        let mut color = GdkRgba::default();
        // SAFETY: self is a live widget and color a valid out-parameter.
        unsafe { sys::gtk_widget_get_color(self.as_ptr(), &mut color) };
        color
    }
}

impl Label {
    pub fn set_markup(&self, markup: &CStr) {
        // SAFETY: self is a live GtkLabel and markup is NUL-terminated; GTK parses and copies it.
        unsafe { sys::gtk_label_set_markup(self.as_ptr(), markup.as_ptr()) };
    }

    pub fn set_max_width_chars(&self, n_chars: c_int) {
        // SAFETY: self is a live GtkLabel.
        unsafe { sys::gtk_label_set_max_width_chars(self.as_ptr(), n_chars) };
    }
}

impl Button {
    pub fn set_label(&self, label: &CStr) {
        // SAFETY: self is a live GtkButton and label is NUL-terminated; GTK copies it.
        unsafe { sys::gtk_button_set_label(self.as_ptr(), label.as_ptr()) };
    }
}

impl Box {
    pub fn remove(&self, child: &impl AsRef<Widget>) {
        // SAFETY: self is a live GtkBox; GTK checks that child is one of its children.
        unsafe { sys::gtk_box_remove(self.as_ptr(), child.as_ref().as_ptr()) };
    }
}

impl Image {
    pub fn set_pixel_size(&self, size: c_int) {
        // SAFETY: self is a live GtkImage.
        unsafe { sys::gtk_image_set_pixel_size(self.as_ptr(), size) };
    }

    pub fn set_from_texture(&self, texture: &Texture) {
        // SAFETY: self is a live GtkImage and texture a live GdkTexture, which is a GdkPaintable the image references.
        unsafe { sys::gtk_image_set_from_paintable(self.as_ptr().cast(), texture.as_ptr().cast()) };
    }
}
