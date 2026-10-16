//! Southstar — the GtkApplication: startup, session recovery and the cache sweep around a run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use core::sync::atomic::{AtomicI32, Ordering};
use std::ffi::CString;

use southstar_gtk_ffi as gtk;

use super::gtk::{borrowed, build_filename, connect, file_test, handler, take_gstring, tr};
use super::procview_media::ns_proc_video_helper_available;
use super::window;
use crate::window as rules;

unsafe extern "C" {
    fn ns_history_init();
    fn ns_history_shutdown();
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const LC_NUMERIC: c_int = 1;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const LC_NUMERIC: c_int = 4;

static INITIAL_WIDTH: AtomicI32 = AtomicI32::new(0);
static INITIAL_HEIGHT: AtomicI32 = AtomicI32::new(0);

pub(super) fn ns_procapp_set_window_size(width: c_int, height: c_int) {
    INITIAL_WIDTH.store(width, Ordering::Relaxed);
    INITIAL_HEIGHT.store(height, Ordering::Relaxed);
}

fn initial_size() -> Option<(c_int, c_int)> {
    let (w, h) = (
        INITIAL_WIDTH.load(Ordering::Relaxed),
        INITIAL_HEIGHT.load(Ordering::Relaxed),
    );
    (w > 0 && h > 0).then_some((w, h))
}

struct Context {
    url: Option<CString>,
    session_path: Option<CString>,
    recover: bool,
    private_mode: bool,
}

unsafe extern "C" fn on_activate(app: *mut c_void, data: *mut c_void) {
    unsafe {
        let ctx = &*data.cast::<Context>();
        gtk::setlocale(LC_NUMERIC, c"C".as_ptr());
        window::install_icon_search_paths();
        super::icons::ns_icon_install_window_icon(c"southstar");
        #[cfg(target_os = "macos")]
        super::macos_dock::set_dock_icon();
        window::install_chrome_css();
        let pw = window::proc_window_new(app, initial_size());
        *pw.session_path.borrow_mut() = ctx.session_path.clone();
        pw.present();
        pw.apply_scheme();

        let mut opened = false;
        if ctx.recover
            && let Some(path) = &ctx.session_path
        {
            let mut contents: *mut c_char = ptr::null_mut();
            if gtk::g_file_get_contents(
                path.as_ptr(),
                &mut contents,
                ptr::null_mut(),
                ptr::null_mut(),
            ) != 0
                && let Some(contents) = take_gstring(contents)
            {
                for line in contents.to_bytes().split(|&c| c == b'\n') {
                    if rules::session_url_recoverable(line) {
                        let url = super::gtk::cstring(line);
                        pw.add_tab(&url, !opened);
                        opened = true;
                    }
                }
            }
            if opened {
                pw.status(tr(
                    c"Recovered the previous session after an unexpected exit",
                ));
            }
        }
        if !opened {
            let url = ctx.url.as_deref().unwrap_or(c"about:start");
            pw.add_tab_full(url, true, ctx.private_mode);
        }
        if pw.session_path.borrow().is_some() {
            pw.session_timer.set(gtk::g_timeout_add_seconds(
                4,
                window::write_session,
                pw.as_data(),
            ));
        }
    }
}

fn mtime_seconds(path: &CStr) -> Option<i64> {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        std::path::Path::new(std::ffi::OsStr::from_bytes(path.to_bytes()))
    };
    #[cfg(not(unix))]
    let path = std::path::Path::new(path.to_str().ok()?);
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    Some(match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs_f64().ceil() as i64),
    })
}

fn clear_cache_dir(name: &CStr, min_age_s: i64) {
    let cache = unsafe { borrowed(gtk::g_get_user_cache_dir()) }.unwrap_or(c"");
    let dir = build_filename(&[cache, c"southstar", name]);
    let cutoff = unsafe { gtk::g_get_real_time() } / 1_000_000 - min_age_s;
    let mut stack: Vec<CString> = vec![dir];
    let mut dirs: Vec<CString> = Vec::new();
    let mut guard = 0;
    while !stack.is_empty() && guard < rules::CACHE_WALK_LIMIT {
        guard += 1;
        let Some(d) = stack.pop() else {
            break;
        };
        unsafe {
            let gd = gtk::g_dir_open(d.as_ptr(), 0, ptr::null_mut());
            if !gd.is_null() {
                while let Some(entry) = borrowed(gtk::g_dir_read_name(gd)) {
                    let child = build_filename(&[&d, entry]);
                    if file_test(&child, gtk::FILE_TEST_IS_SYMLINK)
                        || !file_test(&child, gtk::FILE_TEST_IS_DIR)
                    {
                        if min_age_s <= 0 || mtime_seconds(&child).is_some_and(|m| m < cutoff) {
                            gtk::g_unlink(child.as_ptr());
                        }
                    } else {
                        stack.push(child);
                    }
                }
                gtk::g_dir_close(gd);
            }
        }
        dirs.push(d);
    }
    for d in dirs.iter().skip(1).rev() {
        unsafe { gtk::g_rmdir(d.as_ptr()) };
    }
}

fn clear_http_caches(at_exit: bool) {
    for name in rules::CACHE_OBJECT_DIRS {
        clear_cache_dir(
            name,
            if at_exit {
                0
            } else {
                rules::CACHE_MIN_AGE_SECONDS
            },
        );
    }
    for name in rules::CACHE_STREAM_DIRS {
        clear_cache_dir(name, rules::CACHE_MIN_AGE_SECONDS);
    }
}

pub(super) fn ns_procapp_run(
    startup_url: &CStr,
    session_path: Option<&CStr>,
    recover: bool,
    private_mode: bool,
) -> c_int {
    unsafe {
        if ns_proc_video_helper_available() {
            gtk::g_setenv(c"NS_VIDEO_HELPER".as_ptr(), c"1".as_ptr(), 1);
        }
        clear_http_caches(false);
        if !private_mode {
            ns_history_init();
        }
        let ctx = Context {
            url: Some(startup_url.to_owned()),
            session_path: session_path.map(CStr::to_owned),
            recover,
            private_mode,
        };
        let app = gtk::gtk_application_new(rules::APP_ID.as_ptr(), gtk::APPLICATION_NON_UNIQUE);
        connect(
            app,
            c"activate",
            handler!(on_activate),
            (&ctx as *const Context).cast_mut().cast(),
        );
        let status = gtk::g_application_run(app, 0, ptr::null_mut());
        gtk::g_object_unref(app);
        ns_history_shutdown();
        clear_http_caches(true);
        status
    }
}
