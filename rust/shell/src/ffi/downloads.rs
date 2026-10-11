//! Southstar — the downloads window: recent files in the downloads folder and fetches started from pages.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_long, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_gtk_ffi::{self as gtk, GBoolean, GByteArray, GtkWidget};

use super::gtk::{
    basename, borrowed, build_filename, connect, cstring, file_test, handler, hexpand_spacer,
    label, open_path, set_margins, take_gstring, tr,
};
use crate::downloads;

#[repr(C)]
struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    content_disposition: *mut c_char,
    csp_header: *mut c_char,
    xframe_options: *mut c_char,
    x_content_type_options: *mut c_char,
    cors_allow_origin: *mut c_char,
    refresh: *mut c_char,
    content_language: *mut c_char,
    raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
}

unsafe extern "C" {
    fn ns_net_fetch_blocking(
        url: *const c_char,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> *mut NsResponse;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_security_mark_download_origin(path: *const c_char, url: *const c_char);
    fn g_clear_error(err: *mut *mut c_void);
}

const STATE_KEY: &CStr = c"ns-downloads";

struct Window {
    win: *mut GtkWidget,
    list: *mut GtkWidget,
}

struct Download {
    url: CString,
    path: CString,
    name: CString,
    progress: *mut GtkWidget,
    status: *mut GtkWidget,
    open: *mut GtkWidget,
    pulse: u32,
    ok: bool,
    size: u64,
}

fn downloads_dir() -> &'static CStr {
    let d = unsafe { borrowed(gtk::g_get_user_special_dir(gtk::USER_DIRECTORY_DOWNLOAD)) };
    if let Some(d) = d.filter(|d| !d.is_empty()) {
        return d;
    }
    static FALLBACK: OnceLock<CString> = OnceLock::new();
    FALLBACK.get_or_init(|| {
        let home = unsafe { borrowed(gtk::g_get_home_dir()) }.unwrap_or(c"");
        build_filename(&[home, c"Downloads"])
    })
}

unsafe extern "C" fn on_open(_button: *mut GtkWidget, path: *mut c_void) {
    open_path(unsafe { CStr::from_ptr(path.cast()) });
}

unsafe extern "C" fn free_path(path: *mut c_void, _closure: *mut c_void) {
    unsafe { gtk::g_free(path) };
}

unsafe extern "C" fn on_open_folder(_button: *mut GtkWidget, _data: *mut c_void) {
    open_path(downloads_dir());
}

unsafe extern "C" fn pulse(data: *mut c_void) -> GBoolean {
    let d = data.cast::<Download>();
    unsafe { gtk::gtk_progress_bar_pulse((*d).progress) };
    gtk::SOURCE_CONTINUE
}

unsafe extern "C" fn finish_idle(data: *mut c_void) -> GBoolean {
    let d = unsafe { Box::from_raw(data.cast::<Download>()) };
    unsafe {
        if d.pulse != 0 {
            gtk::g_source_remove(d.pulse);
        }
        gtk::gtk_progress_bar_set_fraction(d.progress, if d.ok { 1.0 } else { 0.0 });
        let detail = if d.ok {
            take_gstring(gtk::g_format_size(d.size)).unwrap_or_default()
        } else {
            tr(c"Failed").to_owned()
        };
        let text = cstring(downloads::finished_label(
            d.name.to_bytes(),
            detail.to_bytes(),
        ));
        gtk::gtk_label_set_text(d.status, text.as_ptr());
        if d.ok {
            gtk::gtk_widget_set_sensitive(d.open, 1);
        }
    }
    gtk::SOURCE_REMOVE
}

unsafe extern "C" fn worker(data: *mut c_void) -> *mut c_void {
    let d = data.cast::<Download>();
    unsafe {
        let mut err = ptr::null_mut();
        let resp = ns_net_fetch_blocking((*d).url.as_ptr(), ptr::null_mut(), &mut err);
        if !resp.is_null()
            && (*resp).error.is_null()
            && !(*resp).body.is_null()
            && gtk::g_file_set_contents(
                (*d).path.as_ptr(),
                (*(*resp).body).data.cast(),
                (*(*resp).body).len as isize,
                ptr::null_mut(),
            ) != 0
        {
            (*d).ok = true;
            (*d).size = u64::from((*(*resp).body).len);
            let origin = if (*resp).final_url.is_null() {
                (*d).url.as_ptr()
            } else {
                (*resp).final_url
            };
            ns_security_mark_download_origin((*d).path.as_ptr(), origin);
        }
        if !resp.is_null() {
            ns_response_free(resp);
        }
        g_clear_error(&mut err);
        gtk::g_idle_add(finish_idle, data);
    }
    ptr::null_mut()
}

unsafe fn row_new(
    name: &CStr,
    done: bool,
    open_path: &CStr,
    d: Option<&mut Download>,
) -> *mut GtkWidget {
    unsafe {
        let row = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 3);
        set_margins(row, 8, 8, 6, 6);
        let status = label(name);
        gtk::gtk_label_set_xalign(status, 0.0);
        gtk::gtk_label_set_ellipsize(status, gtk::ELLIPSIZE_MIDDLE);
        let hb = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 6);
        let open = gtk::gtk_button_new_with_label(tr(c"Open").as_ptr());
        gtk::gtk_widget_set_sensitive(open, GBoolean::from(done));
        gtk::g_signal_connect_data(
            open.cast(),
            c"clicked".as_ptr(),
            handler!(on_open),
            gtk::g_strdup(open_path.as_ptr()).cast(),
            Some(free_path),
            0,
        );
        match d {
            Some(d) => {
                let progress = gtk::gtk_progress_bar_new();
                gtk::gtk_widget_set_hexpand(progress, 1);
                gtk::gtk_widget_set_valign(progress, gtk::ALIGN_CENTER);
                gtk::gtk_box_append(hb, progress);
                d.progress = progress;
                d.status = status;
                d.open = open;
            }
            None => gtk::gtk_box_append(hb, hexpand_spacer()),
        }
        gtk::gtk_box_append(hb, open);
        gtk::gtk_box_append(row, status);
        gtk::gtk_box_append(row, hb);
        row
    }
}

unsafe fn populate_recent(list: *mut GtkWidget) {
    let dir = downloads_dir();
    let mut shown = 0;
    unsafe {
        let gd = gtk::g_dir_open(dir.as_ptr(), 0, ptr::null_mut());
        if !gd.is_null() {
            let mut files: Vec<CString> = Vec::new();
            while files.len() < downloads::RECENT_SCAN_LIMIT {
                let Some(name) = borrowed(gtk::g_dir_read_name(gd)) else {
                    break;
                };
                if !downloads::listed(name.to_bytes()) {
                    continue;
                }
                files.push(build_filename(&[dir, name]));
            }
            gtk::g_dir_close(gd);
            files.sort_by(|a, b| a.to_bytes().cmp(b.to_bytes()));
            for path in &files {
                if shown >= downloads::RECENT_SHOWN_LIMIT {
                    break;
                }
                if !file_test(path, gtk::FILE_TEST_IS_REGULAR) {
                    continue;
                }
                let row = row_new(&basename(path), true, path, None);
                gtk::gtk_list_box_append(list, row);
                shown += 1;
            }
        }
        if shown == 0 {
            let empty = label(tr(c"Nothing downloaded yet"));
            gtk::gtk_widget_add_css_class(empty, c"dim-label".as_ptr());
            gtk::gtk_widget_set_margin_top(empty, 24);
            gtk::gtk_widget_set_margin_bottom(empty, 24);
            gtk::gtk_list_box_append(list, empty);
        }
    }
}

unsafe extern "C" fn on_close_request(win: *mut GtkWidget, _data: *mut c_void) -> GBoolean {
    unsafe { gtk::gtk_widget_set_visible(win, 0) };
    1
}

unsafe extern "C" fn free_window(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<Window>()) });
}

unsafe fn window_for(parent: *mut GtkWidget) -> Option<&'static Window> {
    unsafe {
        gtk::g_object_get_data(parent.cast(), STATE_KEY.as_ptr())
            .cast::<Window>()
            .as_ref()
    }
}

pub unsafe fn show(parent: *mut GtkWidget) -> *mut GtkWidget {
    unsafe {
        if let Some(w) = window_for(parent) {
            gtk::gtk_window_present(w.win);
            return w.list;
        }
        let win = gtk::gtk_window_new();
        gtk::gtk_window_set_title(win, tr(c"Downloads").as_ptr());
        gtk::gtk_window_set_default_size(win, 460, 420);
        gtk::gtk_window_set_transient_for(win, parent);
        connect(
            win,
            c"close-request",
            handler!(on_close_request),
            ptr::null_mut(),
        );

        let vbox = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 0);
        let header = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 6);
        set_margins(header, 8, 8, 8, 4);
        let spacer = hexpand_spacer();
        let folder = gtk::gtk_button_new_with_label(tr(c"Open folder").as_ptr());
        connect(
            folder,
            c"clicked",
            handler!(on_open_folder),
            ptr::null_mut(),
        );
        gtk::gtk_box_append(header, spacer);
        gtk::gtk_box_append(header, folder);

        let scroll = gtk::gtk_scrolled_window_new();
        gtk::gtk_widget_set_vexpand(scroll, 1);
        let list = gtk::gtk_list_box_new();
        gtk::gtk_list_box_set_selection_mode(list, gtk::SELECTION_NONE);
        gtk::gtk_scrolled_window_set_child(scroll, list);
        gtk::gtk_box_append(vbox, header);
        gtk::gtk_box_append(vbox, scroll);
        gtk::gtk_window_set_child(win, vbox);

        let state = Box::into_raw(Box::new(Window { win, list }));
        gtk::g_object_set_data_full(
            parent.cast(),
            STATE_KEY.as_ptr(),
            state.cast(),
            Some(free_window),
        );
        populate_recent(list);
        gtk::gtk_window_present(win);
        list
    }
}

fn target_path(url: &CStr, suggested: Option<&CStr>) -> (CString, CString) {
    let mut name = suggested
        .filter(|s| !s.is_empty())
        .map(basename)
        .unwrap_or_default();
    if !downloads::usable_name(name.to_bytes()) {
        let base = basename(url);
        let base = downloads::strip_query(base.to_bytes());
        name = if downloads::usable_name(base) {
            cstring(base)
        } else {
            cstring(downloads::FALLBACK_NAME)
        };
    }
    let dir = downloads_dir();
    let mut path = build_filename(&[dir, &name]);
    let mut n = 1;
    while file_test(&path, gtk::FILE_TEST_EXISTS) && n < downloads::MAX_ALTERNATIVES {
        let alt = cstring(downloads::numbered(name.to_bytes(), n));
        path = build_filename(&[dir, &alt]);
        n += 1;
    }
    (name, path)
}

pub unsafe fn start(parent: *mut GtkWidget, url: &CStr, suggested: Option<&CStr>) {
    if url.is_empty() {
        return;
    }
    let (name, path) = target_path(url, suggested);
    unsafe {
        let list = show(parent);
        let mut d = Box::new(Download {
            url: url.to_owned(),
            path,
            name,
            progress: ptr::null_mut(),
            status: ptr::null_mut(),
            open: ptr::null_mut(),
            pulse: 0,
            ok: false,
            size: 0,
        });
        let (row_name, row_path) = (d.name.clone(), d.path.clone());
        let row = row_new(&row_name, false, &row_path, Some(&mut d));
        gtk::gtk_list_box_prepend(list, row);
        let d = Box::into_raw(d);
        (*d).pulse = gtk::g_timeout_add(120, pulse, d.cast());
        let t = gtk::g_thread_new(c"ns-download".as_ptr(), worker, d.cast());
        if !t.is_null() {
            gtk::g_thread_unref(t);
        }
    }
}
