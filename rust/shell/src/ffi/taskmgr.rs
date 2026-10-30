//! Southstar — the task manager window: each tab's sandboxed renderer and media helpers, with End task and thread dumps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int, c_long};
use core::ops::ControlFlow;
use std::rc::Rc;

use southstar_glib::{self as glib, SourceId};
use southstar_gtk::{self as gtk, Widget};
use southstar_renderer_client as renderer_client;

use super::gtk::{cstring, tr, tr_string};
use super::view::{self, View};
use crate::taskmgr::{self, CpuHistory};

const WINDOW_KEY: &CStr = c"task-manager";
const PID_KEY: &CStr = c"pid";
const NAME_KEY: &CStr = c"name";
const VIEW_KEY: &CStr = c"view";

struct DumpWindow {
    window: gtk::Window,
    text: gtk::TextView,
}

struct TaskManager {
    notebook: Widget,
    list: gtk::ListBox,
    parent: Widget,
    dump: Option<DumpWindow>,
    timer: Option<SourceId>,
    cpu: CpuHistory,
}

type Shared = Rc<RefCell<TaskManager>>;

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

fn row_pid(row: &Widget) -> c_int {
    row.object().data::<c_int>(PID_KEY).unwrap_or(0)
}

fn fixed_label(text: &str, width: c_int, xalign: f32) -> gtk::Label {
    let l = gtk::Label::new(&cstring(text));
    l.set_width_chars(width);
    l.set_xalign(xalign);
    l
}

struct Row<'a> {
    icon_name: &'a CStr,
    name: String,
    pid: c_int,
    state: &'a str,
    rss: c_long,
    view: Option<View>,
}

fn add_row(tm: &mut TaskManager, row: Row<'_>) {
    let hbox = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 12);
    hbox.set_margins(10, 10, 5, 5);
    let title = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 6);
    title.set_hexpand(true);
    title.append(&gtk::Image::from_icon_name(row.icon_name));
    let l_name = gtk::Label::new(&cstring(row.name.as_str()));
    l_name.set_xalign(0.0);
    l_name.set_ellipsize(gtk::sys::ELLIPSIZE_END);
    l_name.set_hexpand(true);
    title.append(&l_name);

    let pid = row.pid;
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

    hbox.append(&title);
    hbox.append(&fixed_label(&taskmgr::count(pid), 8, 1.0));
    hbox.append(&fixed_label(&taskmgr::threads(threads), 8, 1.0));
    hbox.append(&fixed_label(row.state, 11, 0.0));
    hbox.append(&fixed_label(&taskmgr::memory(row.rss), 10, 1.0));
    hbox.append(&fixed_label(&taskmgr::percent(pct), 7, 1.0));
    hbox.append(&fixed_label(&taskmgr::cpu_time(cpu), 9, 1.0));

    let list_row = gtk::ListBoxRow::new();
    list_row.set_child(&hbox);
    let data = list_row.object();
    data.set_data(VIEW_KEY, row.view);
    data.set_data(PID_KEY, Some(pid));
    data.set_data(NAME_KEY, Some(Rc::<str>::from(row.name)));
    tm.list.append(&list_row);
}

fn tab_rows(tm: &mut TaskManager) {
    for v in view::notebook_views(&tm.notebook) {
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
        add_row(
            tm,
            Row {
                icon_name: c"text-x-generic-symbolic",
                name: format!("{}  —  {tab}", tr_string(c"HTML renderer")),
                pid,
                state: &info.state,
                rss: info.rss,
                view: Some(v),
            },
        );
        let helpers = [
            (
                v.audio_pid(),
                c"audio-volume-high-symbolic",
                c"Audio playback",
            ),
            (v.video_pid(), c"video-x-generic-symbolic", c"Video decoder"),
        ];
        for (helper_pid, icon_name, label) in helpers {
            if helper_pid > 0 {
                let helper = proc_info(helper_pid);
                add_row(
                    tm,
                    Row {
                        icon_name,
                        name: format!("   ⤷ {}", tr_string(label)),
                        pid: helper_pid,
                        state: &helper.state,
                        rss: helper.rss,
                        view: Some(v),
                    },
                );
            }
        }
    }
}

fn refresh(tm: &mut TaskManager) {
    tm.cpu.begin(glib::monotonic_us());
    let selected_pid = tm.list.selected_row().map_or(0, |row| row_pid(&row));
    tm.list.remove_all();

    let brand = tr_string(c"Southstar Browser");
    let wpid = southstar_watchdog::supervisor_pid();
    if wpid > 0 {
        let info = proc_info(wpid);
        add_row(
            tm,
            Row {
                icon_name: c"applications-system-symbolic",
                name: format!("{brand} ({})", tr_string(c"watchdog")),
                pid: wpid,
                state: &info.state,
                rss: info.rss,
                view: None,
            },
        );
    }
    let gpid = renderer_client::self_pid();
    let info = proc_info(gpid);
    add_row(
        tm,
        Row {
            icon_name: c"web-browser-symbolic",
            name: format!("{brand} (GTK frontend)"),
            pid: gpid,
            state: &info.state,
            rss: info.rss,
            view: None,
        },
    );
    tab_rows(tm);

    if selected_pid > 0
        && let Some(row) = tm.list.rows().find(|row| row_pid(row) == selected_pid)
    {
        tm.list.select_row(&row);
    }
    tm.cpu.prune();
}

fn end_task(tm: &Shared) {
    let row = tm.borrow().list.selected_row();
    if let Some(row) = row
        && let Some(v) = row.object().data::<View>(VIEW_KEY)
    {
        let pid = row_pid(&row);
        if pid > 0 && pid == v.video_pid() {
            v.stop_video();
        } else if pid > 0 && pid == v.audio_pid() {
            v.stop_audio();
        } else {
            v.end_task();
        }
    }
    refresh(&mut tm.borrow_mut());
}

fn dump_window(tm: &Shared) -> gtk::TextView {
    if let Some(dump) = &tm.borrow().dump {
        return dump.text.clone();
    }
    let window = gtk::Window::new();
    window.set_title(tr(c"Thread dump"));
    window.set_transient_for(Some(&tm.borrow().parent));
    window.set_default_size(680, 480);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    let text = gtk::TextView::new();
    text.set_editable(false);
    text.set_cursor_visible(false);
    text.set_monospace(true);
    text.set_wrap_mode(gtk::sys::WRAP_NONE);
    text.set_margins(8, 8, 8, 8);
    scroll.set_child(&text);
    window.set_child(&scroll);
    let weak = Rc::downgrade(tm);
    window.connect_destroy(move || {
        if let Some(tm) = weak.upgrade() {
            tm.borrow_mut().dump = None;
        }
    });
    tm.borrow_mut().dump = Some(DumpWindow {
        window,
        text: text.clone(),
    });
    text
}

fn thread_dump(tm: &Shared) {
    let mut out: Vec<u8> = Vec::new();
    let mut dumped = 0;
    let rows: Vec<_> = tm.borrow().list.rows().collect();
    for row in rows {
        let pid = row_pid(&row);
        if pid > 0 {
            let name = row.object().data::<Rc<str>>(NAME_KEY);
            let name = name.as_deref().unwrap_or("process");
            let bytes = southstar_threaddump::dump_text(pid, Some(name.as_bytes()));
            glib::stderr_write(&bytes);
            if dumped > 0 {
                out.push(b'\n');
            }
            out.extend_from_slice(&bytes);
            dumped += 1;
        }
    }
    if dumped == 0 {
        out.extend_from_slice(tr(c"No processes to dump.").to_bytes());
    }
    let text = dump_window(tm);
    text.set_text(&String::from_utf8_lossy(&out));
    if let Some(dump) = &tm.borrow().dump {
        dump.window.present();
    }
}

fn header_label(text: &CStr, width: c_int, xalign: f32, expand: bool) -> gtk::Label {
    let l = gtk::Label::new(tr(text));
    l.set_xalign(xalign);
    if width > 0 {
        l.set_width_chars(width);
    }
    if expand {
        l.set_hexpand(true);
    }
    l.add_css_class(c"heading");
    l
}

fn button(label: &CStr, tm: &Shared, action: fn(&Shared)) -> gtk::Button {
    let b = gtk::Button::with_label(tr(label));
    let weak = Rc::downgrade(tm);
    b.connect_clicked(move || {
        if let Some(tm) = weak.upgrade() {
            action(&tm);
        }
    });
    b
}

pub fn show(parent: &Widget, notebook: &Widget) {
    if let Some(existing) = parent.object().data::<gtk::Window>(WINDOW_KEY) {
        existing.present();
        return;
    }
    let win = gtk::Window::new();
    let title = cstring(format!(
        "{} — {}",
        tr_string(c"Task Manager"),
        tr_string(c"Southstar Browser")
    ));
    win.set_title(&title);
    win.set_transient_for(Some(parent));
    win.set_default_size(720, 380);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::sys::SELECTION_SINGLE);
    let ncpu = std::thread::available_parallelism().map_or(1, |n| n.get());
    let tm: Shared = Rc::new(RefCell::new(TaskManager {
        notebook: notebook.clone(),
        list: list.clone(),
        parent: parent.clone(),
        dump: None,
        timer: None,
        cpu: CpuHistory::new(i32::try_from(ncpu).unwrap_or(i32::MAX)),
    }));

    let vbox = gtk::Box::new(gtk::ORIENTATION_VERTICAL, 0);
    let hdr = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 12);
    hdr.set_margins(10, 10, 8, 4);
    hdr.append(&header_label(c"Task", 0, 0.0, true));
    hdr.append(&header_label(c"Process ID", 8, 1.0, false));
    hdr.append(&header_label(c"Threads", 8, 1.0, false));
    hdr.append(&header_label(c"State", 11, 0.0, false));
    hdr.append(&header_label(c"Memory", 10, 1.0, false));
    hdr.append(&header_label(c"CPU %", 7, 1.0, false));
    hdr.append(&header_label(c"CPU time", 9, 1.0, false));

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(&list);

    let bar = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 6);
    bar.set_margins(10, 10, 6, 8);
    let spacer = gtk::Label::new(c"");
    spacer.set_hexpand(true);
    let end_btn = button(c"End task", &tm, end_task);
    end_btn.add_css_class(c"destructive-action");
    bar.append(&spacer);
    bar.append(&button(c"Thread dump", &tm, thread_dump));
    bar.append(&button(c"Refresh", &tm, |tm| refresh(&mut tm.borrow_mut())));
    bar.append(&end_btn);

    vbox.append(&hdr);
    vbox.append(&scroll);
    vbox.append(&bar);
    win.set_child(&vbox);

    parent.object().set_data(WINDOW_KEY, Some(win.clone()));
    let destroyed = tm.clone();
    let owner = parent.downgrade();
    win.connect_destroy(move || {
        let (timer, dump) = {
            let mut tm = destroyed.borrow_mut();
            (tm.timer.take(), tm.dump.take())
        };
        if let Some(timer) = timer {
            timer.remove();
        }
        if let Some(dump) = dump {
            dump.window.destroy();
        }
        if let Some(owner) = owner.upgrade() {
            owner.object().set_data::<gtk::Window>(WINDOW_KEY, None);
        }
    });

    refresh(&mut tm.borrow_mut());
    let ticking = Rc::downgrade(&tm);
    let timer = glib::timeout_add_local(1500, move || match ticking.upgrade() {
        Some(tm) => {
            refresh(&mut tm.borrow_mut());
            ControlFlow::Continue(())
        }
        None => ControlFlow::Break(()),
    });
    tm.borrow_mut().timer = Some(timer);
    win.present();
}
