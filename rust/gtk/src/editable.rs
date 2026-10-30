//! Southstar — text entries, the search entry and its match signals, and appending to a text view's buffer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};

use southstar_gtk_ffi::{self as sys, GType, GtkTextIter};

use crate::signal::{self, SignalHandlerId};
use crate::widgets::widget_class;
use crate::{TextView, Widget};

unsafe extern "C" {
    fn gtk_search_entry_get_type() -> GType;
}

widget_class!(SearchEntry => gtk_search_entry_get_type);

fn editable_text(editable: &Widget) -> Vec<u8> {
    // SAFETY: callers pass a live GtkEditable; the text belongs to it.
    let text = unsafe { sys::gtk_editable_get_text(editable.as_ptr()) };
    if text.is_null() {
        return Vec::new();
    }
    // SAFETY: GTK returned a NUL-terminated string that stays valid until the text changes.
    unsafe { CStr::from_ptr(text) }.to_bytes().to_vec()
}

fn select_editable_region(editable: &Widget, start: c_int, end: c_int) {
    // SAFETY: callers pass a live GtkEditable; GTK clamps the positions.
    unsafe { sys::gtk_editable_select_region(editable.as_ptr(), start, end) };
}

impl SearchEntry {
    pub fn new() -> SearchEntry {
        // SAFETY: gtk_search_entry_new returns a new floating widget.
        SearchEntry(Widget::adopt(unsafe { sys::gtk_search_entry_new() }))
    }

    pub fn text(&self) -> Vec<u8> {
        editable_text(self)
    }

    pub fn select_region(&self, start: c_int, end: c_int) {
        select_editable_region(self, start, end);
    }

    pub fn connect_search_changed(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"search-changed", f)
    }

    pub fn connect_activate(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"activate", f)
    }

    pub fn connect_next_match(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"next-match", f)
    }

    pub fn connect_previous_match(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"previous-match", f)
    }

    pub fn connect_stop_search(&self, f: impl Fn() + 'static) -> SignalHandlerId {
        signal::connect_plain(self.object(), c"stop-search", f)
    }
}

impl Default for SearchEntry {
    fn default() -> SearchEntry {
        SearchEntry::new()
    }
}

fn empty_iter() -> GtkTextIter {
    GtkTextIter {
        dummy1: core::ptr::null_mut(),
        dummy2: core::ptr::null_mut(),
        dummy3: 0,
        dummy4: 0,
        dummy5: 0,
        dummy6: 0,
        dummy7: 0,
        dummy8: 0,
        dummy9: core::ptr::null_mut(),
        dummy10: core::ptr::null_mut(),
        dummy11: 0,
        dummy12: 0,
        dummy13: 0,
        dummy14: core::ptr::null_mut(),
    }
}

impl TextView {
    fn buffer(&self) -> *mut c_void {
        // SAFETY: self is a live GtkTextView; the buffer it returns is owned by the view.
        unsafe { sys::gtk_text_view_get_buffer(self.as_ptr()) }
    }

    pub fn append(&self, text: &[u8]) {
        let buffer = self.buffer();
        let len = c_int::try_from(text.len()).unwrap_or(c_int::MAX);
        let mut end = empty_iter();
        // SAFETY: buffer is the view's live buffer and end is storage for the iterator.
        unsafe { sys::gtk_text_buffer_get_end_iter(buffer, &mut end) };
        // SAFETY: end is a valid iterator into buffer and text is len readable bytes GTK copies.
        unsafe { sys::gtk_text_buffer_insert(buffer, &mut end, text.as_ptr().cast(), len) };
    }

    pub fn scroll_to_end(&self) {
        let buffer = self.buffer();
        let mut end = empty_iter();
        // SAFETY: buffer is the view's live buffer and end is storage for the iterator.
        unsafe { sys::gtk_text_buffer_get_end_iter(buffer, &mut end) };
        // SAFETY: end is a valid iterator into buffer; the anonymous mark is the buffer's.
        let mark = unsafe { sys::gtk_text_buffer_create_mark(buffer, core::ptr::null(), &end, 0) };
        // SAFETY: self is live and mark is a mark of its buffer.
        unsafe { sys::gtk_text_view_scroll_mark_onscreen(self.as_ptr(), mark) };
        // SAFETY: mark belongs to buffer and is not used again.
        unsafe { sys::gtk_text_buffer_delete_mark(buffer, mark) };
    }
}
