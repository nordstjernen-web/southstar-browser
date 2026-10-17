//! Southstar — the task manager window: each tab's sandboxed renderer and media helpers, with End task and thread dumps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long, c_void};
use core::ptr;

use southstar_gtk_ffi::{self as gtk, GBoolean, GtkWidget};
use southstar_renderer_client as renderer_client;

use super::gtk::{
    borrowed, connect, cstring, handler, hexpand_spacer, label, set_margins, tr, tr_string,
};
use super::view::{self, View};
use crate::taskmgr::{self, CpuHistory};

const WINDOW_KEY: &CStr = c"ns-task-manager";
const PID_KEY: &CStr = c"ns-pid";
const NAME_KEY: &CStr = c"ns-name";
const VIEW_KEY: &CStr = c"ns-view";

struct TaskManager {
    parent: *mut GtkWidget,
    notebook: *mut GtkWidget,
    list: *mut GtkWidget,
    dump_win: *mut GtkWidget,
    dump_view: *mut GtkWidget,
    timer: u32,
    cpu: CpuHistory,
}

struct ProcInfo {
    state: String,
    rss: c_long,
}

fn proc_info(pid: c_int) -> ProcInfo {
    let info = renderer_client::proc_info(pid);
    ProcInfo {
        state: info.state.unwrap_or("running").to_owned(),
        rss: info.rss_kb.unwrap_or(-1),
    }
}

fn row_pid(row: *mut GtkWidget) -> c_int {
    unsafe { gtk::g_object_get_data(row.cast(), PID_KEY.as_ptr()) as isize as c_int }
}

unsafe fn fixed_label(text: &str, width: c_int, xalign: f32) -> *mut GtkWidget {
    unsafe {
        let l = label(&cstring(text));
        gtk::gtk_label_set_width_chars(l, width);
        gtk::gtk_label_set_xalign(l, xalign);
        l
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn add_row(
    tm: &mut TaskManager,
    icon_name: &CStr,
    name: &str,
    pid: c_int,
    state: &str,
    rss: c_long,
    v: Option<View>,
) {
    unsafe {
        let hbox = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 12);
        set_margins(hbox, 10, 10, 5, 5);
        let title = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 6);
        gtk::gtk_widget_set_hexpand(title, 1);
        gtk::gtk_box_append(title, gtk::gtk_image_new_from_icon_name(icon_name.as_ptr()));
        let name_c = cstring(name);
        let l_name = label(&name_c);
        gtk::gtk_label_set_xalign(l_name, 0.0);
        gtk::gtk_label_set_ellipsize(l_name, gtk::ELLIPSIZE_END);
        gtk::gtk_widget_set_hexpand(l_name, 1);
        gtk::gtk_box_append(title, l_name);

        let threads = if pid > 0 {
            renderer_client::proc_threads(pid)
        } else {
            -1
        };
        let cpu = if pid > 0 {
            renderer_client::proc_cpu(pid)
        } else {
            -1.0
        };
        let pct = tm.cpu.pct(pid, cpu);

        gtk::gtk_box_append(hbox, title);
        gtk::gtk_box_append(hbox, fixed_label(&taskmgr::count(pid), 8, 1.0));
        gtk::gtk_box_append(hbox, fixed_label(&taskmgr::threads(threads), 8, 1.0));
        gtk::gtk_box_append(hbox, fixed_label(state, 11, 0.0));
        gtk::gtk_box_append(hbox, fixed_label(&taskmgr::memory(rss), 10, 1.0));
        gtk::gtk_box_append(hbox, fixed_label(&taskmgr::percent(pct), 7, 1.0));
        gtk::gtk_box_append(hbox, fixed_label(&taskmgr::cpu_time(cpu), 9, 1.0));

        let row = gtk::gtk_list_box_row_new();
        gtk::gtk_list_box_row_set_child(row, hbox);
        if let Some(v) = v {
            gtk::g_object_set_data(row.cast(), VIEW_KEY.as_ptr(), v.raw().cast());
        }
        gtk::g_object_set_data(row.cast(), PID_KEY.as_ptr(), pid as isize as *mut c_void);
        gtk::g_object_set_data_full(
            row.cast(),
            NAME_KEY.as_ptr(),
            gtk::g_strdup(name_c.as_ptr()).cast(),
            Some(gtk::g_free),
        );
        gtk::gtk_list_box_append(tm.list, row);
    }
}

unsafe fn refresh(tm: &mut TaskManager) {
    unsafe {
        tm.cpu.begin(gtk::g_get_monotonic_time());
        let sel = gtk::gtk_list_box_get_selected_row(tm.list);
        let selected_pid = if sel.is_null() { 0 } else { row_pid(sel) };
        loop {
            let child = gtk::gtk_widget_get_first_child(tm.list);
            if child.is_null() {
                break;
            }
            gtk::gtk_list_box_remove(tm.list, child);
        }

        let brand = tr_string(c"Southstar Browser");
        let wpid = southstar_watchdog::supervisor_pid();
        if wpid > 0 {
            let info = proc_info(wpid);
            let name = format!("{brand} ({})", tr_string(c"watchdog"));
            add_row(
                tm,
                c"applications-system-symbolic",
                &name,
                wpid,
                &info.state,
                info.rss,
                None,
            );
        }
        let gpid = renderer_client::self_pid();
        let info = proc_info(gpid);
        add_row(
            tm,
            c"web-browser-symbolic",
            &format!("{brand} (GTK frontend)"),
            gpid,
            &info.state,
            info.rss,
            None,
        );

        for v in view::views(tm.notebook) {
            let mut pid = v.renderer_pid();
            let mut info = ProcInfo {
                state: "starting".into(),
                rss: -1,
            };
            if pid > 0 {
                info = proc_info(pid);
            } else if southstar_renderer_host::single_process_enabled() {
                pid = renderer_client::self_pid();
                info = proc_info(pid);
                info.state = "in-process".into();
            }
            let tab = v
                .title()
                .filter(|t| !t.is_empty())
                .or_else(|| v.url().filter(|u| !u.is_empty()))
                .map_or_else(
                    || tr_string(c"New Tab"),
                    |s| s.to_string_lossy().into_owned(),
                );
            let name = format!("{}  —  {tab}", tr_string(c"HTML renderer"));
            add_row(
                tm,
                c"text-x-generic-symbolic",
                &name,
                pid,
                &info.state,
                info.rss,
                Some(v),
            );
            let apid = v.audio_pid();
            if apid > 0 {
                let a = proc_info(apid);
                let name = format!("   ⤷ {}", tr_string(c"Audio playback"));
                add_row(
                    tm,
                    c"audio-volume-high-symbolic",
                    &name,
                    apid,
                    &a.state,
                    a.rss,
                    Some(v),
                );
            }
            let vpid = v.video_pid();
            if vpid > 0 {
                let vi = proc_info(vpid);
                let name = format!("   ⤷ {}", tr_string(c"Video decoder"));
                add_row(
                    tm,
                    c"video-x-generic-symbolic",
                    &name,
                    vpid,
                    &vi.state,
                    vi.rss,
                    Some(v),
                );
            }
        }

        if selected_pid > 0 {
            let mut r = gtk::gtk_widget_get_first_child(tm.list);
            while !r.is_null() {
                if row_pid(r) == selected_pid {
                    gtk::gtk_list_box_select_row(tm.list, r);
                    break;
                }
                r = gtk::gtk_widget_get_next_sibling(r);
            }
        }
        tm.cpu.prune();
    }
}

unsafe extern "C" fn tick(data: *mut c_void) -> GBoolean {
    unsafe { refresh(&mut *data.cast::<TaskManager>()) };
    gtk::SOURCE_CONTINUE
}

unsafe extern "C" fn on_end_task(_button: *mut GtkWidget, data: *mut c_void) {
    unsafe {
        let tm = &mut *data.cast::<TaskManager>();
        let row = gtk::gtk_list_box_get_selected_row(tm.list);
        if !row.is_null()
            && let Some(v) = View::from_raw(gtk::g_object_get_data(row.cast(), VIEW_KEY.as_ptr()))
        {
            let pid = row_pid(row);
            if pid > 0 && pid == v.video_pid() {
                v.stop_video();
            } else if pid > 0 && pid == v.audio_pid() {
                v.stop_audio();
            } else {
                v.end_task();
            }
        }
        refresh(tm);
    }
}

unsafe extern "C" fn on_refresh(_button: *mut GtkWidget, data: *mut c_void) {
    unsafe { refresh(&mut *data.cast::<TaskManager>()) };
}

unsafe extern "C" fn on_dump_destroyed(_win: *mut GtkWidget, data: *mut c_void) {
    let tm = data.cast::<TaskManager>();
    unsafe {
        (*tm).dump_win = ptr::null_mut();
        (*tm).dump_view = ptr::null_mut();
    }
}

unsafe fn show_dump(tm: &mut TaskManager, text: &[u8]) {
    unsafe {
        if tm.dump_win.is_null() {
            let win = gtk::gtk_window_new();
            gtk::gtk_window_set_title(win, tr(c"Thread dump").as_ptr());
            gtk::gtk_window_set_transient_for(win, tm.parent);
            gtk::gtk_window_set_default_size(win, 680, 480);
            let scroll = gtk::gtk_scrolled_window_new();
            gtk::gtk_widget_set_vexpand(scroll, 1);
            let view = gtk::gtk_text_view_new();
            gtk::gtk_text_view_set_editable(view, 0);
            gtk::gtk_text_view_set_cursor_visible(view, 0);
            gtk::gtk_text_view_set_monospace(view, 1);
            gtk::gtk_text_view_set_wrap_mode(view, gtk::WRAP_NONE);
            set_margins(view, 8, 8, 8, 8);
            gtk::gtk_scrolled_window_set_child(scroll, view);
            gtk::gtk_window_set_child(win, scroll);
            tm.dump_win = win;
            tm.dump_view = view;
            connect(
                win,
                c"destroy",
                handler!(on_dump_destroyed),
                (tm as *mut TaskManager).cast(),
            );
        }
        let buf = gtk::gtk_text_view_get_buffer(tm.dump_view);
        let text = cstring(text);
        let valid = gtk::g_utf8_make_valid(text.as_ptr(), -1);
        gtk::gtk_text_buffer_set_text(buf, valid, -1);
        gtk::g_free(valid.cast());
        gtk::gtk_window_present(tm.dump_win);
    }
}

unsafe extern "C" fn on_thread_dump(_button: *mut GtkWidget, data: *mut c_void) {
    unsafe {
        let tm = &mut *data.cast::<TaskManager>();
        let mut out: Vec<u8> = Vec::new();
        let mut dumped = 0;
        let mut r = gtk::gtk_widget_get_first_child(tm.list);
        while !r.is_null() {
            let pid = row_pid(r);
            if pid > 0 {
                let name = borrowed(gtk::g_object_get_data(r.cast(), NAME_KEY.as_ptr()).cast())
                    .unwrap_or(c"process");
                let bytes = southstar_threaddump::dump_text(pid, Some(name.to_bytes()));
                southstar_glib::stderr_write(&bytes);
                if dumped > 0 {
                    out.push(b'\n');
                }
                out.extend_from_slice(&bytes);
                dumped += 1;
            }
            r = gtk::gtk_widget_get_next_sibling(r);
        }
        if dumped == 0 {
            out.extend_from_slice(tr(c"No processes to dump.").to_bytes());
        }
        show_dump(tm, &out);
    }
}

unsafe extern "C" fn on_destroyed(_win: *mut GtkWidget, data: *mut c_void) {
    unsafe {
        let tm = Box::from_raw(data.cast::<TaskManager>());
        if tm.timer != 0 {
            gtk::g_source_remove(tm.timer);
        }
        if !tm.dump_win.is_null() {
            gtk::gtk_window_destroy(tm.dump_win);
        }
        gtk::g_object_set_data(tm.parent.cast(), WINDOW_KEY.as_ptr(), ptr::null_mut());
    }
}

unsafe fn header_label(text: &CStr, width: c_int, xalign: f32, expand: bool) -> *mut GtkWidget {
    unsafe {
        let l = label(tr(text));
        gtk::gtk_label_set_xalign(l, xalign);
        if width > 0 {
            gtk::gtk_label_set_width_chars(l, width);
        }
        if expand {
            gtk::gtk_widget_set_hexpand(l, 1);
        }
        gtk::gtk_widget_add_css_class(l, c"heading".as_ptr());
        l
    }
}

pub unsafe fn show(parent: *mut GtkWidget, notebook: *mut GtkWidget) {
    unsafe {
        let existing = gtk::g_object_get_data(parent.cast(), WINDOW_KEY.as_ptr());
        if !existing.is_null() {
            gtk::gtk_window_present(existing.cast());
            return;
        }
        let win = gtk::gtk_window_new();
        let title = cstring(format!(
            "{} — {}",
            tr_string(c"Task Manager"),
            tr_string(c"Southstar Browser")
        ));
        gtk::gtk_window_set_title(win, title.as_ptr());
        gtk::gtk_window_set_transient_for(win, parent);
        gtk::gtk_window_set_default_size(win, 720, 380);

        let tm = Box::into_raw(Box::new(TaskManager {
            parent,
            notebook,
            list: ptr::null_mut(),
            dump_win: ptr::null_mut(),
            dump_view: ptr::null_mut(),
            timer: 0,
            cpu: CpuHistory::new(gtk::g_get_num_processors() as i32),
        }));

        let vbox = gtk::gtk_box_new(gtk::ORIENTATION_VERTICAL, 0);
        let hdr = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 12);
        set_margins(hdr, 10, 10, 8, 4);
        gtk::gtk_box_append(hdr, header_label(c"Task", 0, 0.0, true));
        gtk::gtk_box_append(hdr, header_label(c"Process ID", 8, 1.0, false));
        gtk::gtk_box_append(hdr, header_label(c"Threads", 8, 1.0, false));
        gtk::gtk_box_append(hdr, header_label(c"State", 11, 0.0, false));
        gtk::gtk_box_append(hdr, header_label(c"Memory", 10, 1.0, false));
        gtk::gtk_box_append(hdr, header_label(c"CPU %", 7, 1.0, false));
        gtk::gtk_box_append(hdr, header_label(c"CPU time", 9, 1.0, false));

        let scroll = gtk::gtk_scrolled_window_new();
        gtk::gtk_widget_set_vexpand(scroll, 1);
        let list = gtk::gtk_list_box_new();
        (*tm).list = list;
        gtk::gtk_list_box_set_selection_mode(list, gtk::SELECTION_SINGLE);
        gtk::gtk_scrolled_window_set_child(scroll, list);

        let bar = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 6);
        set_margins(bar, 10, 10, 6, 8);
        let spacer = hexpand_spacer();
        let dump_btn = gtk::gtk_button_new_with_label(tr(c"Thread dump").as_ptr());
        connect(dump_btn, c"clicked", handler!(on_thread_dump), tm.cast());
        let refresh_btn = gtk::gtk_button_new_with_label(tr(c"Refresh").as_ptr());
        connect(refresh_btn, c"clicked", handler!(on_refresh), tm.cast());
        let end_btn = gtk::gtk_button_new_with_label(tr(c"End task").as_ptr());
        gtk::gtk_widget_add_css_class(end_btn, c"destructive-action".as_ptr());
        connect(end_btn, c"clicked", handler!(on_end_task), tm.cast());
        gtk::gtk_box_append(bar, spacer);
        gtk::gtk_box_append(bar, dump_btn);
        gtk::gtk_box_append(bar, refresh_btn);
        gtk::gtk_box_append(bar, end_btn);

        gtk::gtk_box_append(vbox, hdr);
        gtk::gtk_box_append(vbox, scroll);
        gtk::gtk_box_append(vbox, bar);
        gtk::gtk_window_set_child(win, vbox);

        gtk::g_object_set_data(parent.cast(), WINDOW_KEY.as_ptr(), win.cast());
        connect(win, c"destroy", handler!(on_destroyed), tm.cast());

        refresh(&mut *tm);
        (*tm).timer = gtk::g_timeout_add(1500, tick, tm.cast());
        gtk::gtk_window_present(win);
    }
}
