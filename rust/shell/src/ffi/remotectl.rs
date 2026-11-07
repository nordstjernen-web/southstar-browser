//! Southstar — the opt-in loopback control channel that lets a local MCP client drive the browser window.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::c_int;
use core::ops::ControlFlow;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use southstar_glib::{self as glib, SourceId};
use southstar_mcp::json::{self, Value, quoted};

use super::gtk::cstring;
use super::procview_remote as remote;
use super::view::View;
use super::window;
use crate::remote::{hex, parse_keys, token_equal};

const MAX_CONNS: usize = 4;
const MAX_LINE: usize = 16 * 1024 * 1024;
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_MS: u32 = 50;
const LOAD_TIMEOUT_MS: u64 = 30_000;
const SETTLE_MS: u64 = 2_000;
const SETTLE_MIN_MS: u64 = 120;
const RENDER_MS: u64 = 1_500;
const EVAL_TIMEOUT_MS: u32 = 30_000;
const CONFIG_POLL_SECONDS: u32 = 2;

type Reply = Box<dyn FnOnce(Result<String, String>)>;

struct Conn {
    id: u64,
    stream: TcpStream,
    authed: bool,
}

struct Shared {
    open: AtomicBool,
    token: Mutex<String>,
    conns: Mutex<Vec<Conn>>,
    next_conn: AtomicU64,
}

struct Server {
    shared: Arc<Shared>,
    port: u16,
}

#[derive(Default)]
struct State {
    inited: bool,
    requested: bool,
    config_enabled: bool,
    server: Option<Server>,
    client_name: Option<String>,
    config_timer: Option<SourceId>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub fn request() {
    STATE.with(|s| s.borrow_mut().requested = true);
}

fn control_file() -> PathBuf {
    southstar_mcp::control_file_path()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn authed_count(shared: &Shared) -> usize {
    lock(&shared.conns).iter().filter(|c| c.authed).count()
}

fn update_indicator() {
    let status = STATE.with(|s| {
        let s = s.borrow();
        let server = s.server.as_ref()?;
        (authed_count(&server.shared) > 0).then(|| s.client_name.clone().unwrap_or_default())
    });
    window::set_remote_status(status.as_deref());
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(contents)?;
    drop(file);
    std::fs::rename(&tmp, path)
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

fn write_control(json: &str) -> bool {
    let path = control_file();
    match write_private(&path, json.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            glib::warning(&format!(
                "remote control: cannot write {}: {e}",
                path.display()
            ));
            false
        }
    }
}

fn remove_control_file() {
    let path = control_file();
    let mine = std::fs::read(&path)
        .ok()
        .and_then(|bytes| json::parse(&bytes))
        .and_then(|info| info.num("pid"))
        .is_some_and(|pid| pid as u32 == std::process::id());
    if mine {
        let _ = std::fs::remove_file(&path);
    }
}

fn make_token() -> Option<String> {
    let mut raw = [0u8; 32];
    southstar_sandbox::csprng_fill(&mut raw).then(|| hex(&raw))
}

fn reply_line(id: &str, outcome: Result<String, String>) -> String {
    match outcome {
        Ok(result) => {
            let result = if result.is_empty() {
                "{}".into()
            } else {
                result
            };
            format!("{{\"id\":{id},\"ok\":true,\"result\":{result}}}\n")
        }
        Err(e) => format!("{{\"id\":{id},\"ok\":false,\"error\":{}}}\n", quoted(&e)),
    }
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let n = reader
        .take(MAX_LINE as u64 + 1)
        .read_until(b'\n', &mut line)
        .ok()?;
    if n == 0 || line.len() > MAX_LINE {
        return None;
    }
    while matches!(line.last(), Some(b'\n' | b'\r' | b' ')) {
        line.pop();
    }
    Some(line)
}

fn mark_authed(shared: &Shared, id: u64) {
    for c in lock(&shared.conns).iter_mut() {
        if c.id == id {
            c.authed = true;
        }
    }
    glib::idle_add_once(update_indicator);
}

fn serve_connection(shared: Arc<Shared>, id: u64, stream: TcpStream) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    let mut reader = BufReader::new(read_half);
    let _ = writer.set_read_timeout(Some(AUTH_TIMEOUT));
    let deadline = Instant::now() + AUTH_TIMEOUT;
    let mut authed = false;
    while shared.open.load(Ordering::SeqCst) {
        if !authed && Instant::now() >= deadline {
            break;
        }
        let Some(line) = read_request(&mut reader) else {
            break;
        };
        if line.is_empty() {
            continue;
        }
        let req = json::parse(&line);
        let id_json = req
            .as_ref()
            .and_then(|r| r.get("id"))
            .map_or_else(|| "null".to_owned(), Value::to_json);
        let Some(req) = req.filter(Value::is_object) else {
            let _ =
                writer.write_all(reply_line(&id_json, Err("malformed request".into())).as_bytes());
            continue;
        };
        let token_ok = {
            let token = lock(&shared.token);
            shared.open.load(Ordering::SeqCst)
                && token_equal(req.str("token").unwrap_or(""), &token)
        };
        if !token_ok {
            let _ = writer.write_all(reply_line(&id_json, Err("bad token".into())).as_bytes());
            break;
        }
        if !authed {
            authed = true;
            let _ = writer.set_read_timeout(None);
            mark_authed(&shared, id);
        }
        let (tx, rx) = mpsc::channel();
        glib::idle_add_once(move || {
            dispatch(
                req,
                Box::new(move |outcome| {
                    let _ = tx.send(outcome);
                }),
            );
        });
        let Ok(outcome) = rx.recv() else {
            break;
        };
        if writer
            .write_all(reply_line(&id_json, outcome).as_bytes())
            .is_err()
        {
            break;
        }
    }
    let _ = writer.shutdown(Shutdown::Both);
    lock(&shared.conns).retain(|c| c.id != id);
    glib::idle_add_once(update_indicator);
}

fn accept_loop(shared: Arc<Shared>, listener: TcpListener) {
    for stream in listener.incoming() {
        if !shared.open.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else {
            continue;
        };
        let Ok(handle) = stream.try_clone() else {
            continue;
        };
        let id = shared.next_conn.fetch_add(1, Ordering::SeqCst);
        {
            let mut conns = lock(&shared.conns);
            if conns.len() >= MAX_CONNS {
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            conns.push(Conn {
                id,
                stream: handle,
                authed: false,
            });
        }
        let shared = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("ns-remote-conn".into())
            .spawn(move || serve_connection(shared, id, stream));
        if spawned.is_err() {
            glib::warning("remote control: cannot start a connection thread");
        }
    }
}

fn start() {
    if STATE.with(|s| s.borrow().server.is_some()) {
        return;
    }
    let Some(token) = make_token() else {
        glib::warning("remote control: no random source for the token");
        return;
    };
    let listener = match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))) {
        Ok(l) => l,
        Err(e) => {
            glib::warning(&format!("remote control: cannot listen on 127.0.0.1: {e}"));
            return;
        }
    };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else {
        return;
    };
    let shared = Arc::new(Shared {
        open: AtomicBool::new(true),
        token: Mutex::new(token.clone()),
        conns: Mutex::new(Vec::new()),
        next_conn: AtomicU64::new(1),
    });
    let info = format!(
        "{{\"port\":{port},\"token\":\"{token}\",\"pid\":{},\"version\":{}}}\n",
        std::process::id(),
        quoted(crate::titlerow::VERSION)
    );
    if !write_control(&info) {
        return;
    }
    let accept_shared = Arc::clone(&shared);
    let spawned = std::thread::Builder::new()
        .name("ns-remote-accept".into())
        .spawn(move || accept_loop(accept_shared, listener));
    if spawned.is_err() {
        remove_control_file();
        return;
    }
    STATE.with(|s| s.borrow_mut().server = Some(Server { shared, port }));
    update_indicator();
}

fn close_all(user_stopped: bool) {
    let server = STATE.with(|s| s.borrow_mut().server.take());
    if let Some(server) = server {
        server.shared.open.store(false, Ordering::SeqCst);
        lock(&server.shared.token).clear();
        for c in lock(&server.shared.conns).drain(..) {
            let _ = c.stream.shutdown(Shutdown::Both);
        }
        let wake = SocketAddr::from((Ipv4Addr::LOCALHOST, server.port));
        let _ = TcpStream::connect_timeout(&wake, Duration::from_millis(500));
    }
    if user_stopped {
        write_control(&format!(
            "{{\"stopped\":true,\"pid\":{}}}\n",
            std::process::id()
        ));
    } else {
        remove_control_file();
    }
    update_indicator();
}

fn config_enabled() -> bool {
    let config = southstar_config::lock();
    config.reload();
    config.get().remote_control != 0
}

fn poll_config() {
    let on = config_enabled();
    let changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let changed = on != s.config_enabled;
        s.config_enabled = on;
        changed
    });
    if !changed {
        return;
    }
    if on {
        start();
    } else {
        close_all(false);
    }
}

pub fn init() {
    let first = STATE.with(|s| !core::mem::replace(&mut s.borrow_mut().inited, true));
    if !first {
        return;
    }
    let on = config_enabled();
    let requested = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.config_enabled = on;
        s.requested
    });
    let timer = glib::timeout_add_seconds_local(CONFIG_POLL_SECONDS, || {
        poll_config();
        ControlFlow::Continue(())
    });
    STATE.with(|s| s.borrow_mut().config_timer = Some(timer));
    if requested || on {
        start();
    }
}

pub fn stop_by_user() {
    close_all(true);
}

pub fn shutdown() {
    close_all(false);
    if let Some(timer) = STATE.with(|s| s.borrow_mut().config_timer.take()) {
        timer.remove();
    }
}

fn view_state(out: &mut String, v: View) {
    let url = v
        .url()
        .map(|u| u.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = v
        .title()
        .map(|t| t.to_string_lossy().into_owned())
        .unwrap_or_default();
    let g = remote::geometry(v);
    let _ = write!(
        out,
        "\"url\":{},\"title\":{},\"loading\":{},\"viewport\":{{\"width\":{},\"height\":{},\"scroll_x\":{},\"scroll_y\":{}}},\"zoom\":{}",
        quoted(&url),
        quoted(&title),
        v.is_loading(),
        g.width,
        g.height,
        g.scroll_x,
        g.scroll_y,
        (g.scale * 100.0).round() as i64
    );
}

fn state_json(v: View) -> String {
    let mut s = format!("{{\"tab\":{},", window::remote_tab_id(v));
    view_state(&mut s, v);
    s.push('}');
    s
}

fn tabs_json() -> String {
    let mut s = String::from("{\"tabs\":[");
    for (i, t) in window::remote_tabs().iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "{{\"tab\":{},\"window\":{},\"active\":{},\"private\":{},",
            t.id,
            t.window,
            t.active,
            t.view.is_private()
        );
        view_state(&mut s, t.view);
        s.push('}');
    }
    s.push_str("]}");
    s
}

#[derive(Clone, Copy, PartialEq)]
enum Wait {
    Settle,
    Load,
    Render,
}

struct Op {
    reply: RefCell<Option<Reply>>,
    tab: Cell<c_int>,
    timeout: Cell<Option<SourceId>>,
}

impl Op {
    fn done(&self) -> bool {
        self.reply.borrow().is_none()
    }

    fn finish(&self, outcome: Result<String, String>) {
        if let Some(timer) = self.timeout.take() {
            timer.remove();
        }
        if let Some(reply) = self.reply.borrow_mut().take() {
            reply(outcome);
        }
    }

    fn view(&self) -> Option<View> {
        window::remote_tab_view(self.tab.get())
    }

    fn reply_state(&self) {
        match self.view() {
            Some(v) => self.finish(Ok(state_json(v))),
            None => self.finish(Err("the tab was closed".into())),
        }
    }

    fn screenshot_now(&self) {
        let Some(v) = self.view() else {
            self.finish(Err("the tab was closed".into()));
            return;
        };
        let Some((png, w, h)) = remote::screenshot(v) else {
            self.finish(Err(
                "could not capture the tab (is the window minimised?)".into()
            ));
            return;
        };
        let mut s = format!(
            "{{\"tab\":{},\"width\":{w},\"height\":{h},\"png\":\"{}\",",
            window::remote_tab_id(v),
            base64(&png)
        );
        view_state(&mut s, v);
        s.push('}');
        self.finish(Ok(s));
    }
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn start_wait(op: Rc<Op>, kind: Wait, max_ms: u64) {
    let started = Instant::now();
    let mut deadline = started + Duration::from_millis(max_ms);
    let mut saw_loading = false;
    glib::timeout_add_local(POLL_MS, move || {
        if op.done() {
            return ControlFlow::Break(());
        }
        let Some(v) = op.view() else {
            op.finish(Err("the tab was closed".into()));
            return ControlFlow::Break(());
        };
        let now = Instant::now();
        let loading = v.is_loading();
        if loading && !saw_loading {
            saw_loading = true;
            deadline = deadline.max(now + Duration::from_millis(LOAD_TIMEOUT_MS));
        }
        let settled = !loading
            && !remote::rendering(v)
            && now.duration_since(started) >= Duration::from_millis(SETTLE_MIN_MS);
        if !settled && now < deadline {
            return ControlFlow::Continue(());
        }
        if kind == Wait::Render {
            op.screenshot_now();
        } else {
            op.reply_state();
        }
        ControlFlow::Break(())
    });
}

fn settle(op: Rc<Op>, v: View) {
    arm_timeout(&op);
    remote::eval(
        v,
        b"0",
        Box::new(move |_| {
            if !op.done() {
                start_wait(op, Wait::Settle, SETTLE_MS);
            }
        }),
    );
}

fn arm_timeout(op: &Rc<Op>) {
    let weak = Rc::downgrade(op);
    let timer = glib::timeout_add_local_once(EVAL_TIMEOUT_MS, move || {
        if let Some(op) = weak.upgrade() {
            op.timeout.set(None);
            op.finish(Err("timed out waiting for the page".into()));
        }
    });
    op.timeout.set(Some(timer));
}

fn text_reply(op: Rc<Op>) -> remote::TextFn {
    Box::new(move |text| match text {
        Some(text) => {
            let text = String::from_utf8_lossy(&text).into_owned();
            op.finish(Ok(format!("{{\"text\":{}}}", quoted(&text))));
        }
        None => op.finish(Err(
            "the page did not answer (it may be loading or busy)".into()
        )),
    })
}

fn set_client(name: Option<&str>) {
    let name = name
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().take(64).collect::<String>());
    STATE.with(|s| s.borrow_mut().client_name = name);
    update_indicator();
}

fn dispatch(req: Value, reply: Reply) {
    let op = Rc::new(Op {
        reply: RefCell::new(Some(reply)),
        tab: Cell::new(req.num("tab").map_or(0, |t| t as c_int)),
        timeout: Cell::new(None),
    });
    if STATE.with(|s| s.borrow().server.is_none()) {
        op.finish(Err("remote control is off".into()));
        return;
    }
    let cmd = req.str("cmd").unwrap_or("");
    match cmd {
        "hello" => {
            set_client(req.str("client"));
            op.finish(Ok(format!(
                "{{\"version\":{},\"pid\":{}}}",
                quoted(crate::titlerow::VERSION),
                std::process::id()
            )));
            return;
        }
        "tabs" => {
            op.finish(Ok(tabs_json()));
            return;
        }
        "tab_new" => {
            let url = req
                .str("url")
                .filter(|u| !u.is_empty())
                .unwrap_or("about:blank");
            let id = window::remote_tab_new(&cstring(url));
            if id <= 0 || window::remote_tab_view(id).is_none() {
                op.finish(Err("could not open a tab".into()));
                return;
            }
            op.tab.set(id);
            start_wait(op, Wait::Load, LOAD_TIMEOUT_MS);
            return;
        }
        _ => {}
    }
    let Some(v) = op.view() else {
        let msg = if op.tab.get() > 0 {
            "no tab with that id"
        } else {
            "no open tab"
        };
        op.finish(Err(msg.into()));
        return;
    };
    op.tab.set(window::remote_tab_id(v));
    match cmd {
        "tab_close" => {
            if window::remote_tab_close(op.tab.get()) {
                op.finish(Ok("{}".into()));
            } else {
                op.finish(Err("cannot close the last tab".into()));
            }
            return;
        }
        "tab_select" => {
            window::remote_tab_select(op.tab.get(), true);
            op.reply_state();
            return;
        }
        "state" => {
            op.reply_state();
            return;
        }
        "eval" => {
            let Some(js) = req.str("js") else {
                op.finish(Err("missing js".into()));
                return;
            };
            arm_timeout(&op);
            remote::eval(v, js.as_bytes(), text_reply(op));
            return;
        }
        "console" => {
            arm_timeout(&op);
            remote::console(v, text_reply(op));
            return;
        }
        "wait_load" => {
            let ms = req
                .num("ms")
                .unwrap_or(LOAD_TIMEOUT_MS as f64)
                .clamp(0.0, 120_000.0);
            start_wait(op, Wait::Load, ms as u64);
            return;
        }
        _ => {}
    }
    window::remote_tab_select(op.tab.get(), false);
    match cmd {
        "navigate" => {
            let Some(url) = req.str("url").filter(|u| !u.is_empty()) else {
                op.finish(Err("missing url".into()));
                return;
            };
            if url.eq_ignore_ascii_case("back") {
                if !v.can_back() {
                    op.finish(Err("no earlier page in this tab's history".into()));
                    return;
                }
                v.back();
            } else if url.eq_ignore_ascii_case("forward") {
                if !v.can_forward() {
                    op.finish(Err("no later page in this tab's history".into()));
                    return;
                }
                v.forward();
            } else if url.eq_ignore_ascii_case("reload") {
                v.reload();
            } else {
                window::remote_tab_load(op.tab.get(), &cstring(url));
            }
            start_wait(op, Wait::Load, LOAD_TIMEOUT_MS);
            return;
        }
        "screenshot" => {
            remote::refresh(v);
            start_wait(op, Wait::Render, RENDER_MS);
            return;
        }
        _ => {}
    }
    let xy = req.num("x").zip(req.num("y"));
    let page = req.flag("page");
    let ok = match cmd {
        "click" => {
            let count = req.num("count").unwrap_or(1.0) as i32;
            let mods = req.num("mods").unwrap_or(0.0) as i32;
            xy.is_some_and(|(x, y)| remote::click(v, x, y, page, count, mods))
        }
        "hover" => xy.is_some_and(|(x, y)| remote::hover(v, x, y, page)),
        "scroll" => {
            let dx = req.num("dx").unwrap_or(0.0);
            let dy = req.num("dy").unwrap_or(0.0);
            xy.is_some_and(|(x, y)| remote::scroll(v, x, y, dx, dy))
        }
        "text" => req.str("text").is_some_and(|t| remote::text(v, t)),
        "key" => {
            let Some(keys) = req.str("keys").filter(|k| !k.is_empty()) else {
                op.finish(Err("missing keys".into()));
                return;
            };
            match parse_keys(keys) {
                Ok(combos) => combos
                    .into_iter()
                    .all(|(keyval, mods)| remote::key(v, keyval, mods)),
                Err(e) => {
                    op.finish(Err(e));
                    return;
                }
            }
        }
        _ => {
            op.finish(Err(format!("unknown command '{cmd}'")));
            return;
        }
    };
    if !ok {
        op.finish(Err(
            "the page is not ready for input yet (or arguments are missing)".into(),
        ));
        return;
    }
    settle(op, v);
}
