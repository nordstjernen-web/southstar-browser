//! Southstar — the GtkApplication: startup, session recovery and the cache sweep around a run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::sync::atomic::{AtomicI32, Ordering};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use southstar_gtk::{Application, CssProvider, Display};
use southstar_gtk_ffi as gtk;

use super::appmain::{dirname, self_exe_str, setenv};
use super::gtk::{build_filename, cstring, tr};
use super::procview_media::ns_proc_video_helper_available;
use super::window;
use crate::window as rules;

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

fn install_icon_search_paths() {
    let Some(theme) = Display::default_display().and_then(|d| d.icon_theme()) else {
        return;
    };
    theme.add_resource_path(c"/org/southstar/WebBrowser/icons");
    let Some(dir) = self_exe_str().and_then(dirname) else {
        return;
    };
    for rel in rules::ICON_SEARCH_PATHS {
        theme.add_search_path(&build_filename(&[&dir, rel]));
    }
}

fn install_chrome_css() {
    let Some(display) = Display::default_display() else {
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_string(&cstring(crate::chrome::CHROME_CSS));
    display.add_style_provider(
        &provider,
        southstar_gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn use_c_numeric_locale() {
    // SAFETY: the category is LC_NUMERIC and the locale a NUL-terminated literal; this runs on the main thread before any window exists.
    unsafe { gtk::setlocale(LC_NUMERIC, c"C".as_ptr()) };
}

fn recovered_session(path: &CStr) -> Vec<CString> {
    let Some(contents) = southstar_glib::file_get_contents(path.to_bytes()) else {
        return Vec::new();
    };
    let text = contents.split(|&c| c == 0).next().unwrap_or_default();
    text.split(|&c| c == b'\n')
        .filter(|line| rules::session_url_recoverable(line))
        .map(cstring)
        .collect()
}

fn on_activate(ctx: &Context, app: &Application) {
    use_c_numeric_locale();
    install_icon_search_paths();
    super::icons::install_window_icon(c"southstar");
    #[cfg(target_os = "macos")]
    super::macos_dock::set_dock_icon();
    install_chrome_css();
    let pw = window::proc_window_new(app, initial_size());
    *pw.session_path.borrow_mut() = ctx.session_path.clone();
    pw.present();
    pw.apply_scheme();

    let mut opened = false;
    if ctx.recover
        && let Some(path) = &ctx.session_path
    {
        for url in recovered_session(path) {
            pw.add_tab(&url, !opened);
            opened = true;
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
    pw.start_session_timer();
}

fn mtime_seconds(path: &Path) -> Option<i64> {
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    Some(match modified.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs_f64().ceil() as i64),
    })
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn clear_cache_dir(name: &CStr, min_age_s: i64) {
    let Some(cache) = southstar_glib::path_from_bytes(&southstar_glib::user_cache_dir()) else {
        return;
    };
    let Some(name) = southstar_glib::path_from_bytes(name.to_bytes()) else {
        return;
    };
    let dir = cache.join("southstar").join(name);
    let cutoff = now_seconds() - min_age_s;
    let mut stack: Vec<PathBuf> = vec![dir];
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut guard = 0;
    while !stack.is_empty() && guard < rules::CACHE_WALK_LIMIT {
        guard += 1;
        let Some(d) = stack.pop() else {
            break;
        };
        if let Ok(entries) = std::fs::read_dir(&d) {
            for entry in entries.flatten() {
                let child = entry.path();
                let is_link = std::fs::symlink_metadata(&child).is_ok_and(|m| m.is_symlink());
                if is_link || !child.is_dir() {
                    if min_age_s <= 0 || mtime_seconds(&child).is_some_and(|m| m < cutoff) {
                        let _ = std::fs::remove_file(&child);
                    }
                } else {
                    stack.push(child);
                }
            }
        }
        dirs.push(d);
    }
    for d in dirs.iter().skip(1).rev() {
        let _ = std::fs::remove_dir(d);
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
    if ns_proc_video_helper_available() {
        setenv(c"NS_VIDEO_HELPER", c"1");
    }
    clear_http_caches(false);
    southstar_net::init();
    if !private_mode {
        southstar_history::init();
    }
    let ctx = Context {
        url: Some(startup_url.to_owned()),
        session_path: session_path.map(CStr::to_owned),
        recover,
        private_mode,
    };
    let app = Application::new(rules::APP_ID, southstar_gtk::APPLICATION_NON_UNIQUE);
    app.connect_activate(move |app| on_activate(&ctx, app));
    let status = app.run();
    drop(app);
    southstar_history::shutdown();
    southstar_net::shutdown();
    clear_http_caches(true);
    status
}
