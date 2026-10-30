//! Southstar — the downloads window: recent files in the downloads folder and fetches started from pages.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::CStr;
use core::ops::ControlFlow;
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;
use std::sync::OnceLock;

use southstar_glib::{self as glib, SourceId};
use southstar_gtk::{self as gtk, Widget};

use super::gtk::{basename, build_filename, cstring, file_test, open_path, tr};
use crate::downloads;

const STATE_KEY: &CStr = c"downloads";

#[derive(Clone)]
struct DownloadsWindow {
    window: gtk::Window,
    list: gtk::ListBox,
}

struct Fetch {
    url: CString,
    top_url: Option<CString>,
    path: CString,
}

struct Row {
    name: CString,
    progress: gtk::ProgressBar,
    status: gtk::Label,
    open: gtk::Button,
    pulse: Cell<Option<SourceId>>,
}

thread_local! {
    static RUNNING: RefCell<HashMap<u64, Rc<Row>>> = RefCell::new(HashMap::new());
}

fn downloads_dir() -> &'static CStr {
    static DIR: OnceLock<CString> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = glib::download_dir();
        if !dir.is_empty() {
            return CString::new(dir).unwrap_or_default();
        }
        let home = CString::new(glib::home_dir()).unwrap_or_default();
        build_filename(&[&home, c"Downloads"])
    })
}

fn finish(id: u64, size: Option<u64>) {
    let Some(row) = RUNNING.with_borrow_mut(|running| running.remove(&id)) else {
        return;
    };
    if let Some(pulse) = row.pulse.take() {
        pulse.remove();
    }
    row.progress
        .set_fraction(if size.is_some() { 1.0 } else { 0.0 });
    let detail = match size {
        Some(size) => glib::format_size(size),
        None => tr(c"Failed").to_bytes().to_vec(),
    };
    let text = cstring(downloads::finished_label(row.name.to_bytes(), &detail));
    row.status.set_text(&text);
    if size.is_some() {
        row.open.set_sensitive(true);
    }
}

fn fetch(fetch: &Fetch) -> Option<u64> {
    let resp = southstar_net::download_blocking(
        &fetch.url,
        fetch.top_url.as_deref(),
        downloads::MAX_BYTES,
    )?;
    if resp.error().is_some() {
        return None;
    }
    let body = resp.body()?;
    let path = glib::path_from_bytes(fetch.path.to_bytes())?;
    std::fs::write(path, body).ok()?;
    let origin = resp.final_url().unwrap_or(&fetch.url);
    southstar_sandbox::mark_download_origin(&fetch.path, Some(origin.to_bytes()));
    Some(body.len() as u64)
}

fn begin(row: Rc<Row>, job: Fetch) {
    static NEXT_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let progress = row.progress.clone();
    row.pulse.set(Some(glib::timeout_add_local(120, move || {
        progress.pulse();
        ControlFlow::Continue(())
    })));
    RUNNING.with_borrow_mut(|running| running.insert(id, row));
    let started = std::thread::Builder::new()
        .name("ns-download".into())
        .spawn(move || {
            let size = fetch(&job);
            glib::idle_add_once(move || finish(id, size));
        });
    if started.is_err() {
        finish(id, None);
    }
}

fn open_button(path: &CStr, done: bool) -> gtk::Button {
    let open = gtk::Button::with_label(tr(c"Open"));
    open.set_sensitive(done);
    let path = path.to_owned();
    open.connect_clicked(move || open_path(&path));
    open
}

fn status_label(text: &CStr) -> gtk::Label {
    let status = gtk::Label::new(text);
    status.set_xalign(0.0);
    status.set_ellipsize(gtk::sys::ELLIPSIZE_MIDDLE);
    status
}

fn row_box(status: &gtk::Label, controls: &gtk::Box) -> gtk::Box {
    let row = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 3);
    row.set_margins(8, 8, 6, 6);
    row.append(status);
    row.append(controls);
    row
}

fn recent_row(path: &CStr) -> gtk::Box {
    let status = status_label(&basename(path));
    let controls = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 6);
    let spacer = gtk::Label::new(c"");
    spacer.set_hexpand(true);
    controls.append(&spacer);
    controls.append(&open_button(path, true));
    row_box(&status, &controls)
}

fn download_row(job: Fetch, name: CString) -> gtk::Box {
    let consent = downloads::needs_consent(name.to_bytes());
    let heading = if consent {
        let host = southstar_url::bytes::host_from(job.url.to_bytes()).unwrap_or_default();
        cstring(downloads::consent_label(name.to_bytes(), &host))
    } else {
        name.clone()
    };
    let status = status_label(&heading);
    let controls = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 6);
    let progress = gtk::ProgressBar::new();
    progress.set_hexpand(true);
    progress.set_valign(gtk::sys::ALIGN_CENTER);
    controls.append(&progress);
    let open = open_button(&job.path, false);
    let row = Rc::new(Row {
        name,
        progress,
        status: status.clone(),
        open: open.clone(),
        pulse: Cell::new(None),
    });
    if consent {
        let save = gtk::Button::with_label(tr(c"Save"));
        save.add_css_class(c"suggested-action");
        save.set_tooltip_text(Some(tr(
            c"This file type can run programs on your computer",
        )));
        let job = RefCell::new(Some(job));
        let button = save.downgrade();
        save.connect_clicked(move || {
            let Some(job) = job.borrow_mut().take() else {
                return;
            };
            if let Some(button) = button.upgrade() {
                button.set_visible(false);
            }
            row.status.set_text(&row.name);
            begin(row.clone(), job);
        });
        controls.append(&save);
    } else {
        begin(row, job);
    }
    controls.append(&open);
    row_box(&status, &controls)
}

fn recent_files() -> Vec<CString> {
    let dir = downloads_dir();
    let Some(entries) =
        glib::path_from_bytes(dir.to_bytes()).and_then(|d| std::fs::read_dir(d).ok())
    else {
        return Vec::new();
    };
    let mut files: Vec<CString> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            CString::new(glib::path_bytes(std::path::Path::new(&entry.file_name()))).ok()
        })
        .filter(|name| downloads::listed(name.to_bytes()))
        .take(downloads::RECENT_SCAN_LIMIT)
        .map(|name| build_filename(&[dir, &name]))
        .collect();
    files.sort_by(|a, b| a.to_bytes().cmp(b.to_bytes()));
    files
}

fn populate_recent(list: &gtk::ListBox) {
    let mut shown = 0;
    for path in recent_files() {
        if shown >= downloads::RECENT_SHOWN_LIMIT {
            break;
        }
        if !file_test(&path, gtk::sys::FILE_TEST_IS_REGULAR) {
            continue;
        }
        list.append(&recent_row(&path));
        shown += 1;
    }
    if shown == 0 {
        let empty = gtk::Label::new(tr(c"Nothing downloaded yet"));
        empty.add_css_class(c"dim-label");
        empty.set_margins(0, 0, 24, 24);
        list.append(&empty);
    }
}

fn window_for(parent: &Widget) -> DownloadsWindow {
    if let Some(w) = parent.object().data::<DownloadsWindow>(STATE_KEY) {
        w.window.present();
        return w;
    }
    let window = gtk::Window::new();
    window.set_title(tr(c"Downloads"));
    window.set_default_size(460, 420);
    window.set_transient_for(Some(parent));
    window.set_hide_on_close(true);

    let vbox = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 0);
    let header = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 6);
    header.set_margins(8, 8, 8, 4);
    let spacer = gtk::Label::new(c"");
    spacer.set_hexpand(true);
    let folder = gtk::Button::with_label(tr(c"Open folder"));
    folder.connect_clicked(|| open_path(downloads_dir()));
    header.append(&spacer);
    header.append(&folder);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::sys::SELECTION_NONE);
    scroll.set_child(&list);
    vbox.append(&header);
    vbox.append(&scroll);
    window.set_child(&vbox);

    let w = DownloadsWindow { window, list };
    parent.object().set_data(STATE_KEY, Some(w.clone()));
    populate_recent(&w.list);
    w.window.present();
    w
}

pub fn show(parent: &Widget) {
    window_for(parent);
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
    while file_test(&path, gtk::sys::FILE_TEST_EXISTS) && n < downloads::MAX_ALTERNATIVES {
        let alt = cstring(downloads::numbered(name.to_bytes(), n));
        path = build_filename(&[dir, &alt]);
        n += 1;
    }
    (name, path)
}

pub fn start(parent: &Widget, url: &CStr, suggested: Option<&CStr>, top_url: Option<&CStr>) {
    if url.is_empty() {
        return;
    }
    let (name, path) = target_path(url, suggested);
    let w = window_for(parent);
    let job = Fetch {
        url: url.to_owned(),
        top_url: top_url.map(CStr::to_owned),
        path,
    };
    w.list.prepend(&download_row(job, name));
}
