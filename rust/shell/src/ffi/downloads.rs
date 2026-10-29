//! Southstar — the downloads window: recent files in the downloads folder and fetches started from pages.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_gtk_ffi::{self as gtk, GBoolean, GtkWidget};

use super::gtk::{
    basename, borrowed, build_filename, connect, cstring, file_test, handler, hexpand_spacer,
    label, open_path, set_margins, take_gstring, tr,
};
use crate::downloads;

const STATE_KEY: &CStr = c"ns-downloads";

struct Window {
    win: *mut GtkWidget,
    list: *mut GtkWidget,
}

struct Download {
    url: CString,
    top_url: Option<CString>,
    path: CString,
    name: CString,
    progress: *mut GtkWidget,
    status: *mut GtkWidget,
    open: *mut GtkWidget,
    save: *mut GtkWidget,
    pulse: u32,
    ok: bool,
    size: u64,
}

fn downloads_dir() -> &'static CStr {
    static DIR: OnceLock<CString> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = southstar_glib::download_dir();
        if !dir.is_empty() {
            return CString::new(dir).unwrap_or_default();
        }
        let home = CString::new(southstar_glib::home_dir()).unwrap_or_default();
        build_filename(&[&home, c"Downloads"])
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
        let resp = southstar_net::download_blocking(
            &(*d).url,
            (*d).top_url.as_deref(),
            downloads::MAX_BYTES,
        );
        let body = resp
            .as_ref()
            .filter(|r| r.error().is_none())
            .and_then(southstar_net::Response::body);
        if let (Some(resp), Some(body)) = (&resp, body)
            && gtk::g_file_set_contents(
                (*d).path.as_ptr(),
                body.as_ptr().cast(),
                body.len() as isize,
                ptr::null_mut(),
            ) != 0
        {
            (*d).ok = true;
            (*d).size = body.len() as u64;
            let origin = resp.final_url().unwrap_or(&(*d).url);
            southstar_sandbox::mark_download_origin(&(*d).path, Some(origin.to_bytes()));
        }
        drop(resp);
        gtk::g_idle_add(finish_idle, data);
    }
    ptr::null_mut()
}

unsafe extern "C" fn on_save(button: *mut GtkWidget, data: *mut c_void) {
    let d = data.cast::<Download>();
    // SAFETY: data is the leaked Download the Save button was connected with; finish_idle frees it only after the download that begins here is done.
    let status = unsafe { (*d).status };
    // SAFETY: the same live Download; name is a CString it owns.
    let name = unsafe { (*d).name.as_ptr() };
    // SAFETY: button is the live GtkButton that emitted clicked.
    unsafe { gtk::gtk_widget_set_visible(button, 0) };
    // SAFETY: status is the row's live GtkLabel and name is NUL-terminated.
    unsafe { gtk::gtk_label_set_text(status, name) };
    begin(d);
}

fn begin(d: *mut Download) {
    // SAFETY: d is a leaked Download that finish_idle frees once the worker is done; the pulse timeout stops before that.
    let source = unsafe { gtk::g_timeout_add(120, pulse, d.cast()) };
    // SAFETY: d is live, see above.
    unsafe { (*d).pulse = source };
    // SAFETY: the worker takes d and hands it on to finish_idle, which frees it.
    let thread = unsafe { gtk::g_thread_new(c"ns-download".as_ptr(), worker, d.cast()) };
    if !thread.is_null() {
        // SAFETY: thread is the non-null GThread g_thread_new returned.
        unsafe { gtk::g_thread_unref(thread) };
    }
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
        let consent = d
            .as_ref()
            .is_some_and(|d| downloads::needs_consent(d.name.to_bytes()));
        let heading = match d.as_ref().filter(|_| consent) {
            Some(d) => {
                let host = southstar_url::bytes::host_from(d.url.to_bytes()).unwrap_or_default();
                cstring(downloads::consent_label(name.to_bytes(), &host))
            }
            None => name.to_owned(),
        };
        let status = label(&heading);
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
                if consent {
                    let save = gtk::gtk_button_new_with_label(tr(c"Save").as_ptr());
                    gtk::gtk_widget_add_css_class(save, c"suggested-action".as_ptr());
                    gtk::gtk_widget_set_tooltip_text(
                        save,
                        tr(c"This file type can run programs on your computer").as_ptr(),
                    );
                    connect(save, c"clicked", handler!(on_save), ptr::from_mut(d).cast());
                    gtk::gtk_box_append(hb, save);
                    d.save = save;
                }
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
    let from_page = suggested
        .filter(|s| !s.is_empty())
        .map(basename)
        .and_then(|s| downloads::safe_name(s.to_bytes()));
    let from_url = || {
        let base = basename(url);
        downloads::safe_name(downloads::strip_query(base.to_bytes()))
    };
    let name = cstring(
        from_page
            .or_else(from_url)
            .unwrap_or_else(|| downloads::FALLBACK_NAME.to_vec()),
    );
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

pub unsafe fn start(
    parent: *mut GtkWidget,
    url: &CStr,
    suggested: Option<&CStr>,
    top_url: Option<&CStr>,
) {
    if url.is_empty() {
        return;
    }
    let (name, path) = target_path(url, suggested);
    unsafe {
        let list = show(parent);
        let d = Box::into_raw(Box::new(Download {
            url: url.to_owned(),
            top_url: top_url.map(CStr::to_owned),
            path,
            name,
            progress: ptr::null_mut(),
            status: ptr::null_mut(),
            open: ptr::null_mut(),
            save: ptr::null_mut(),
            pulse: 0,
            ok: false,
            size: 0,
        }));
        let (row_name, row_path) = ((*d).name.clone(), (*d).path.clone());
        let row = row_new(&row_name, false, &row_path, Some(&mut *d));
        gtk::gtk_list_box_prepend(list, row);
        if (*d).save.is_null() {
            begin(d);
        }
    }
}
