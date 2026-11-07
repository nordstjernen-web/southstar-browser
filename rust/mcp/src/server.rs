//! Southstar — the `--mcp` stdio Model Context Protocol server, which forwards each tool call to the browser window over its loopback control channel.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::json::{self, Value, quoted};

const PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_LINE: usize = 64 * 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(150);
const LAUNCH_WAIT_MS: u64 = 45_000;
const JS_TIMEOUT: Duration = Duration::from_secs(30);
const WAIT_MAX_MS: f64 = 30_000.0;
const SCROLL_TICK_PX: f64 = 100.0;

#[derive(Default)]
struct ToolResult {
    text: String,
    image: Option<String>,
    error: bool,
}

impl ToolResult {
    fn add(&mut self, text: &str) {
        self.text.push_str(text);
    }

    fn fail(&mut self, text: &str) {
        self.text.push_str(text);
        self.error = true;
    }
}

struct Channel {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    token: String,
}

struct Mcp {
    self_exe: Option<PathBuf>,
    client_name: Option<String>,
    channel: Option<Channel>,
    next_id: u64,
}

type ToolFn = fn(&mut Mcp, &Value, &mut ToolResult);

struct Tool {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    schema: &'static str,
    run: ToolFn,
}

fn read_line(reader: &mut impl BufRead, limit: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let n = reader.take(limit as u64 + 1).read_until(b'\n', &mut line)?;
    if n == 0 {
        return Ok(None);
    }
    if line.len() > limit {
        return Err(std::io::Error::other("message too large"));
    }
    while matches!(line.last(), Some(b'\n' | b'\r' | b' ')) {
        line.pop();
    }
    Ok(Some(line))
}

#[cfg(unix)]
fn file_is_private_to_user(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let Some(dir) = path.parent() else {
        return false;
    };
    let probe = dir.join(format!(".uid-probe-{}", std::process::id()));
    let mine = std::fs::File::create(&probe)
        .and_then(|f| f.metadata())
        .map(|m| m.uid());
    let _ = std::fs::remove_file(&probe);
    meta.file_type().is_file()
        && mine.is_ok_and(|uid| uid == meta.uid())
        && meta.mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn file_is_private_to_user(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

fn read_control_file(check_private: bool) -> Option<Value> {
    let path = crate::control_file_path();
    if check_private && !file_is_private_to_user(&path) {
        return None;
    }
    json::parse(&std::fs::read(&path).ok()?)
}

fn stopped_by_user() -> bool {
    read_control_file(false).is_some_and(|info| {
        info.flag("stopped")
            && info
                .num("pid")
                .is_some_and(|pid| crate::process_alive(pid as i32))
    })
}

impl Mcp {
    fn drop_channel(&mut self) {
        if let Some(ch) = self.channel.take() {
            let _ = ch.stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn roundtrip(&mut self, cmd: &str, tab: i64, fields: &str) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let Some(ch) = self.channel.as_mut() else {
            return Err("not connected".into());
        };
        let mut line = String::from("{\"token\":");
        json::quote(&mut line, &ch.token);
        let _ = write!(line, ",\"id\":{id},\"cmd\":");
        json::quote(&mut line, cmd);
        if tab > 0 {
            let _ = write!(line, ",\"tab\":{tab}");
        }
        if !fields.is_empty() {
            line.push(',');
            line.push_str(fields);
        }
        line.push_str("}\n");
        let reply = ch
            .stream
            .write_all(line.as_bytes())
            .and_then(|()| read_line(&mut ch.reader, MAX_LINE));
        let reply = match reply {
            Ok(Some(reply)) => reply,
            Ok(None) => {
                self.drop_channel();
                return Err("the browser closed the connection".into());
            }
            Err(e) => {
                self.drop_channel();
                return Err(e.to_string());
            }
        };
        match json::parse(&reply).filter(Value::is_object) {
            Some(v) => Ok(v),
            None => {
                self.drop_channel();
                Err("malformed reply from the browser".into())
            }
        }
    }

    fn try_file(&mut self) -> bool {
        let Some(info) = read_control_file(true) else {
            return false;
        };
        let (Some(port), Some(token)) = (info.num("port"), info.str("token")) else {
            return false;
        };
        if !(1.0..=65535.0).contains(&port) {
            return false;
        }
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port as u16));
        let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(5)) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        let Ok(read_half) = stream.try_clone() else {
            return false;
        };
        self.channel = Some(Channel {
            stream,
            reader: BufReader::new(read_half),
            token: token.to_owned(),
        });
        let fields = format!(
            "\"client\":{}",
            quoted(self.client_name.as_deref().unwrap_or(""))
        );
        let ok = self
            .roundtrip("hello", 0, &fields)
            .is_ok_and(|r| r.flag("ok"));
        if !ok {
            self.drop_channel();
        }
        ok
    }

    fn launch_gui(&self) -> bool {
        let Some(exe) = &self.self_exe else {
            return false;
        };
        let mut cmd = Command::new(exe);
        cmd.arg("--remote-control")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
            let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
            cmd.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB);
            if cmd.spawn().is_ok() {
                return true;
            }
            cmd.creation_flags(flags);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        cmd.spawn().is_ok()
    }

    fn ensure(&mut self) -> Result<(), String> {
        if self.channel.is_some() || self.try_file() {
            return Ok(());
        }
        if stopped_by_user() {
            return Err("remote control was stopped from the Southstar window. Ask the user to turn \"Allow remote control\" back on in Settings, or to restart Southstar".into());
        }
        if !self.launch_gui() {
            return Err(
                "Southstar is not running with remote control enabled, and it could not be started"
                    .into(),
            );
        }
        let deadline = Instant::now() + Duration::from_millis(LAUNCH_WAIT_MS);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(250));
            if self.try_file() {
                return Ok(());
            }
        }
        Err("started Southstar, but its remote-control channel did not come up (is the Settings switch or --remote-control being refused?)".into())
    }

    fn call(&mut self, cmd: &str, tab: i64, fields: &str) -> Result<Value, String> {
        let mut last = String::from("connection lost");
        for _ in 0..2 {
            self.ensure()?;
            match self.roundtrip(cmd, tab, fields) {
                Ok(resp) if resp.flag("ok") => return Ok(resp),
                Ok(resp) => {
                    return Err(resp
                        .str("error")
                        .unwrap_or("the browser refused the request")
                        .to_owned());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn page_call(&mut self, tab: i64, call: &str) -> Result<String, String> {
        let js = format!("({}).{}", crate::PAGE_HELPER, call);
        let resp = self.call("eval", tab, &format!("\"js\":{}", quoted(&js)))?;
        let text = result(&resp)
            .and_then(|r| r.str("text"))
            .ok_or_else(|| "the page returned nothing".to_owned())?;
        match text.strip_prefix("error: ") {
            Some(e) => Err(e.to_owned()),
            None => Ok(text.to_owned()),
        }
    }
}

fn result(resp: &Value) -> Option<&Value> {
    resp.get("result")
}

fn arg_tab(args: &Value) -> i64 {
    args.num("tab_id").map_or(0, |t| t as i64)
}

fn add_state(r: &mut ToolResult, state: Option<&Value>) {
    let Some(state) = state else {
        return;
    };
    let title = state.str("title").unwrap_or("");
    let url = state.str("url").unwrap_or("");
    let tab = state.num("tab").unwrap_or(0.0) as i64;
    let sep = if title.is_empty() { "" } else { " - " };
    r.add(&format!("\nTab {tab}: {url}{sep}{title}"));
    if state.flag("loading") {
        r.add(" (still loading)");
    }
}

#[derive(Default)]
struct Target {
    x: f64,
    y: f64,
    page: bool,
    vx: f64,
    vy: f64,
}

fn resolve_target(
    m: &mut Mcp,
    args: &Value,
    tab: i64,
    r: &mut ToolResult,
    required: bool,
) -> Option<Target> {
    if let Some(reference) = args.str("ref").filter(|s| !s.is_empty()) {
        let point = match m.page_call(tab, &format!("point({})", quoted(reference))) {
            Ok(text) => json::parse(text.as_bytes()),
            Err(e) => {
                r.fail(&e);
                return None;
            }
        };
        let Some(pt) = point else {
            r.fail("could not locate the element");
            return None;
        };
        let target = Target {
            x: pt.num("px").unwrap_or(0.0),
            y: pt.num("py").unwrap_or(0.0),
            page: true,
            vx: pt.num("x").unwrap_or(0.0),
            vy: pt.num("y").unwrap_or(0.0),
        };
        r.add(&format!("Target: {}", pt.str("desc").unwrap_or(reference)));
        if let Some(cover) = pt.str("covered_by") {
            r.add(&format!(
                "\nWarning: the element's centre is covered by {cover}"
            ));
        }
        return Some(target);
    }
    let coord = args.get("coordinate").and_then(Value::as_array);
    let xy = match coord {
        Some([x, y]) => x.as_num().zip(y.as_num()),
        _ => None,
    }
    .or_else(|| args.num("x").zip(args.num("y")));
    if let Some((x, y)) = xy {
        r.add(&format!("Target: ({}, {})", x as i64, y as i64));
        return Some(Target {
            x,
            y,
            page: false,
            vx: x,
            vy: y,
        });
    }
    if required {
        r.fail("give either ref (from read_page/find) or x and y (viewport pixels, as in the screenshot)");
    }
    None
}

fn parse_mods(s: Option<&str>) -> i64 {
    let lower = s.unwrap_or("").to_ascii_lowercase();
    let mut mods = 0;
    if lower.contains("shift") {
        mods |= 1;
    }
    if lower.contains("ctrl") || lower.contains("control") {
        mods |= 2;
    }
    if lower.contains("alt") {
        mods |= 4;
    }
    if lower.contains("meta") || lower.contains("cmd") {
        mods |= 8;
    }
    mods
}

fn do_click(m: &mut Mcp, args: &Value, r: &mut ToolResult, count: i64) {
    let tab = arg_tab(args);
    let Some(t) = resolve_target(m, args, tab, r, true) else {
        return;
    };
    let fields = format!(
        "\"x\":{},\"y\":{},\"page\":{},\"count\":{count},\"mods\":{}",
        t.x as i64,
        t.y as i64,
        t.page,
        parse_mods(args.str("modifiers"))
    );
    match m.call("click", tab, &fields) {
        Ok(resp) => {
            if count == 2 {
                let _ = m.page_call(tab, &format!("dblclick({},{})", t.vx as i64, t.vy as i64));
            }
            r.add(if count == 2 {
                "\nDouble-clicked."
            } else {
                "\nClicked."
            });
            add_state(r, result(&resp));
        }
        Err(e) => r.fail(&format!("\n{e}")),
    }
}

fn tool_click(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    do_click(m, args, r, 1);
}

fn tool_double_click(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    do_click(m, args, r, 2);
}

fn tool_hover(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let tab = arg_tab(args);
    let Some(t) = resolve_target(m, args, tab, r, true) else {
        return;
    };
    let fields = format!(
        "\"x\":{},\"y\":{},\"page\":{}",
        t.x as i64, t.y as i64, t.page
    );
    match m.call("hover", tab, &fields) {
        Ok(resp) => {
            r.add("\nHovering.");
            add_state(r, result(&resp));
        }
        Err(e) => r.fail(&format!("\n{e}")),
    }
}

fn tool_type(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let tab = arg_tab(args);
    let Some(text) = args.str("text") else {
        r.fail("text is required");
        return;
    };
    if args.str("ref").is_some_and(|s| !s.is_empty()) {
        let Some(t) = resolve_target(m, args, tab, r, true) else {
            return;
        };
        let fields = format!(
            "\"x\":{},\"y\":{},\"page\":true,\"count\":1",
            t.x as i64, t.y as i64
        );
        if let Err(e) = m.call("click", tab, &fields) {
            r.fail(&format!("\n{e}"));
            return;
        }
        r.add("\n");
    }
    match m.call("text", tab, &format!("\"text\":{}", quoted(text))) {
        Ok(resp) => {
            r.add(&format!("Typed {} characters.", text.chars().count()));
            add_state(r, result(&resp));
        }
        Err(e) => r.fail(&e),
    }
}

fn tool_key(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let tab = arg_tab(args);
    let Some(keys) = args
        .str("keys")
        .or_else(|| args.str("text"))
        .filter(|k| !k.is_empty())
    else {
        r.fail("keys is required, e.g. \"Enter\", \"ctrl+a\" or \"Tab Tab Enter\"");
        return;
    };
    let n = args.num("repeat").unwrap_or(1.0).clamp(1.0, 100.0) as usize;
    let seq = vec![keys; n].join(" ");
    match m.call("key", tab, &format!("\"keys\":{}", quoted(&seq))) {
        Ok(resp) => {
            r.add(&format!(
                "Pressed {keys}{}.",
                if n > 1 { " repeatedly" } else { "" }
            ));
            add_state(r, result(&resp));
        }
        Err(e) => r.fail(&e),
    }
}

fn tool_scroll(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let tab = arg_tab(args);
    let dir = args.str("direction").filter(|d| !d.is_empty());
    let target = resolve_target(m, args, tab, r, false);
    if r.error {
        return;
    }
    let Some(dir) = dir else {
        if target.is_some() && args.str("ref").is_some() {
            r.add("\nScrolled the element into view.");
        } else {
            r.fail("direction is required (up, down, left or right)");
        }
        return;
    };
    let have = target.is_some();
    let (vx, vy) = match target {
        Some(t) => (t.vx, t.vy),
        None => match m.call("state", tab, "") {
            Ok(st) => {
                let vp = result(&st).and_then(|s| s.get("viewport"));
                let w = vp.and_then(|v| v.num("width")).unwrap_or(800.0);
                let h = vp.and_then(|v| v.num("height")).unwrap_or(600.0);
                (w / 2.0, h / 2.0)
            }
            Err(e) => {
                r.fail(&e);
                return;
            }
        },
    };
    let px = args.num("amount").unwrap_or(3.0).clamp(0.1, 100.0) * SCROLL_TICK_PX;
    let (dx, dy) = match dir.to_ascii_lowercase().as_str() {
        "down" => (0.0, px),
        "up" => (0.0, -px),
        "right" => (px, 0.0),
        "left" => (-px, 0.0),
        _ => {
            r.fail("direction must be up, down, left or right");
            return;
        }
    };
    let fields = format!(
        "\"x\":{},\"y\":{},\"dx\":{},\"dy\":{}",
        vx as i64, vy as i64, dx as i64, dy as i64
    );
    match m.call("scroll", tab, &fields) {
        Ok(resp) => {
            let vp = result(&resp).and_then(|s| s.get("viewport"));
            let sx = vp.and_then(|v| v.num("scroll_x")).unwrap_or(0.0);
            let sy = vp.and_then(|v| v.num("scroll_y")).unwrap_or(0.0);
            r.add(&format!(
                "{}Scrolled {dir} by {} px; scroll position is now {},{}.",
                if have { "\n" } else { "" },
                px as i64,
                sx as i64,
                sy as i64
            ));
        }
        Err(e) => r.fail(&e),
    }
}

fn page_tool(m: &mut Mcp, args: &Value, r: &mut ToolResult, call: &str) {
    match m.page_call(arg_tab(args), call) {
        Ok(text) => r.add(&text),
        Err(e) => r.fail(&e),
    }
}

fn tool_form_input(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let (Some(reference), Some(value)) = (args.str("ref"), args.get("value")) else {
        r.fail("ref and value are required");
        return;
    };
    let call = format!("setValue({},{})", quoted(reference), value.to_json());
    page_tool(m, args, r, &call);
}

fn tool_read_page(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let filter = args
        .str("filter")
        .filter(|f| !f.is_empty())
        .unwrap_or("all");
    let max = args.num("max_chars").unwrap_or(40000.0) as i64;
    page_tool(m, args, r, &format!("readPage({},{max})", quoted(filter)));
}

fn tool_find(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let Some(query) = args.str("query").filter(|q| !q.is_empty()) else {
        r.fail("query is required");
        return;
    };
    let limit = args.num("limit").unwrap_or(20.0) as i64;
    page_tool(m, args, r, &format!("find({},{limit})", quoted(query)));
}

fn tool_get_page_text(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let max = args.num("max_chars").unwrap_or(100_000.0) as i64;
    page_tool(m, args, r, &format!("pageText({max})"));
}

fn tool_javascript(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let tab = arg_tab(args);
    let Some(code) = args
        .str("code")
        .or_else(|| args.str("text"))
        .filter(|c| !c.is_empty())
    else {
        r.fail("code is required");
        return;
    };
    let deadline = Instant::now() + JS_TIMEOUT;
    let mut text = m.page_call(tab, &format!("run({})", quoted(code)));
    loop {
        let v = match text {
            Ok(t) => json::parse(t.as_bytes()).unwrap_or(Value::Null),
            Err(e) => {
                r.fail(&e);
                return;
            }
        };
        if let Some(pending) = v.num("pending") {
            if Instant::now() > deadline {
                r.fail(&format!(
                    "the promise did not settle within {} s",
                    JS_TIMEOUT.as_secs()
                ));
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
            text = m.page_call(tab, &format!("poll({})", pending as i64));
            continue;
        }
        match v.str("error") {
            Some(e) => r.fail(e),
            None => r.add(v.str("value").unwrap_or("undefined")),
        }
        return;
    }
}

fn tool_console(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    match m.call("console", arg_tab(args), "") {
        Ok(resp) => {
            let text = result(&resp).and_then(|v| v.str("text")).unwrap_or("");
            r.add(if text.is_empty() {
                "The console has no new messages."
            } else {
                text
            });
        }
        Err(e) => r.fail(&e),
    }
}

fn tool_screenshot(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    match m.call("screenshot", arg_tab(args), "") {
        Ok(resp) => {
            let res = result(&resp);
            let Some(png) = res.and_then(|v| v.str("png")) else {
                r.fail("no image");
                return;
            };
            r.image = Some(png.to_owned());
            let w = res.and_then(|v| v.num("width")).unwrap_or(0.0) as i64;
            let h = res.and_then(|v| v.num("height")).unwrap_or(0.0) as i64;
            r.add(&format!("Screenshot {w}x{h}. Its pixels are CSS viewport pixels: pass them as x/y to click, hover and scroll."));
            add_state(r, res);
        }
        Err(e) => r.fail(&e),
    }
}

fn nav_call(m: &mut Mcp, args: &Value, r: &mut ToolResult, url: &str, done: &str) {
    match m.call(
        "navigate",
        arg_tab(args),
        &format!("\"url\":{}", quoted(url)),
    ) {
        Ok(resp) => {
            r.add(done);
            add_state(r, result(&resp));
        }
        Err(e) => r.fail(&e),
    }
}

fn tool_navigate(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let Some(url) = args.str("url").filter(|u| !u.is_empty()) else {
        r.fail("url is required (or \"back\" / \"forward\")");
        return;
    };
    let url = url.to_owned();
    nav_call(m, args, r, &url, "Navigated.");
}

fn tool_reload(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    nav_call(m, args, r, "reload", "Reloaded.");
}

fn tool_tabs_list(m: &mut Mcp, _args: &Value, r: &mut ToolResult) {
    let resp = match m.call("tabs", 0, "") {
        Ok(resp) => resp,
        Err(e) => {
            r.fail(&e);
            return;
        }
    };
    let tabs = result(&resp)
        .and_then(|v| v.get("tabs"))
        .and_then(Value::as_array)
        .unwrap_or_default();
    r.add(&format!(
        "{} open tab{}:",
        tabs.len(),
        if tabs.len() == 1 { "" } else { "s" }
    ));
    for t in tabs {
        let title = t.str("title").unwrap_or("");
        r.add(&format!(
            "\n- tab_id {} (window {}){}{}{}: {}{}{}",
            t.num("tab").unwrap_or(0.0) as i64,
            t.num("window").unwrap_or(0.0) as i64,
            if t.flag("active") { " [active]" } else { "" },
            if t.flag("private") { " [private]" } else { "" },
            if t.flag("loading") { " [loading]" } else { "" },
            t.str("url").unwrap_or(""),
            if title.is_empty() { "" } else { " - " },
            title
        ));
    }
}

fn tool_tabs_create(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let url = args
        .str("url")
        .filter(|u| !u.is_empty())
        .unwrap_or("about:blank");
    match m.call("tab_new", 0, &format!("\"url\":{}", quoted(url))) {
        Ok(resp) => {
            r.add("Opened a new tab.");
            add_state(r, result(&resp));
        }
        Err(e) => r.fail(&e),
    }
}

fn tool_tabs_close(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    match m.call("tab_close", arg_tab(args), "") {
        Ok(_) => r.add("Closed the tab."),
        Err(e) => r.fail(&e),
    }
}

fn tool_tab_select(m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let tab = arg_tab(args);
    if tab <= 0 {
        r.fail("tab_id is required (see tabs_list)");
        return;
    }
    match m.call("tab_select", tab, "") {
        Ok(resp) => {
            r.add("Selected.");
            add_state(r, result(&resp));
        }
        Err(e) => r.fail(&e),
    }
}

fn tool_wait(_m: &mut Mcp, args: &Value, r: &mut ToolResult) {
    let ms = args
        .num("ms")
        .or_else(|| args.num("seconds").map(|s| s * 1000.0))
        .unwrap_or(1000.0)
        .clamp(0.0, WAIT_MAX_MS) as u64;
    std::thread::sleep(Duration::from_millis(ms));
    r.add(&format!("Waited {ms} ms."));
}

macro_rules! tab_prop {
    () => {
        "\"tab_id\":{\"type\":\"integer\",\"description\":\"Tab to act on (from tabs_list). Defaults to the active tab.\"}"
    };
}

macro_rules! ref_prop {
    () => {
        "\"ref\":{\"type\":\"string\",\"description\":\"Element reference such as ref_12 from read_page or find.\"}"
    };
}

macro_rules! xy_props {
    () => {
        "\"x\":{\"type\":\"number\",\"description\":\"Viewport x in screenshot pixels.\"},\"y\":{\"type\":\"number\",\"description\":\"Viewport y in screenshot pixels.\"},\"coordinate\":{\"type\":\"array\",\"items\":{\"type\":\"number\"},\"minItems\":2,\"maxItems\":2,\"description\":\"[x, y] viewport pixels, alternative to x and y.\"}"
    };
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "tabs_list",
        title: "List tabs",
        description: "List the open browser tabs with their tab_id, URL, title and which one is active.",
        schema: "{\"type\":\"object\",\"properties\":{}}",
        run: tool_tabs_list,
    },
    Tool {
        name: "tabs_create",
        title: "Open tab",
        description: "Open a new foreground tab, optionally at a URL, and wait for it to load.",
        schema: "{\"type\":\"object\",\"properties\":{\"url\":{\"type\":\"string\",\"description\":\"Address or search text. Defaults to about:blank.\"}}}",
        run: tool_tabs_create,
    },
    Tool {
        name: "tabs_close",
        title: "Close tab",
        description: "Close a tab (the active one by default).",
        schema: concat!("{\"type\":\"object\",\"properties\":{", tab_prop!(), "}}"),
        run: tool_tabs_close,
    },
    Tool {
        name: "tab_select",
        title: "Select tab",
        description: "Bring a tab to the front of its window.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{",
            tab_prop!(),
            "},\"required\":[\"tab_id\"]}"
        ),
        run: tool_tab_select,
    },
    Tool {
        name: "navigate",
        title: "Navigate",
        description: "Load a URL in a tab and wait for the page to finish loading. Pass \"back\" or \"forward\" to move through the tab's history. Text that is not a URL is searched with the default search engine.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"url\":{\"type\":\"string\",\"description\":\"URL, search text, \\\"back\\\" or \\\"forward\\\".\"},",
            tab_prop!(),
            "},\"required\":[\"url\"]}"
        ),
        run: tool_navigate,
    },
    Tool {
        name: "reload",
        title: "Reload",
        description: "Reload the page in a tab and wait for it to load.",
        schema: concat!("{\"type\":\"object\",\"properties\":{", tab_prop!(), "}}"),
        run: tool_reload,
    },
    Tool {
        name: "read_page",
        title: "Read page",
        description: "Return an accessibility-tree-like outline of the page: roles, accessible names, states and viewport bounding boxes (CSS pixels), with stable refs such as ref_12 for interactive elements that the other tools accept. filter=\"interactive\" lists only links, buttons, fields and other controls.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"filter\":{\"type\":\"string\",\"enum\":[\"all\",\"interactive\"],\"description\":\"all (default) or interactive.\"},\"max_chars\":{\"type\":\"integer\",\"description\":\"Truncate the outline after this many characters (default 40000).\"},",
            tab_prop!(),
            "}}"
        ),
        run: tool_read_page,
    },
    Tool {
        name: "find",
        title: "Find elements",
        description: "Find elements by CSS selector or by visible text, accessible name, placeholder or value. Returns matching elements with refs.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"query\":{\"type\":\"string\",\"description\":\"CSS selector or text to look for (case-insensitive).\"},\"limit\":{\"type\":\"integer\",\"description\":\"Maximum matches (default 20).\"},",
            tab_prop!(),
            "},\"required\":[\"query\"]}"
        ),
        run: tool_find,
    },
    Tool {
        name: "click",
        title: "Click",
        description: "Left-click an element (by ref, scrolled into view first) or a viewport point (x, y in screenshot pixels) with real mouse input, then wait for any navigation to settle.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{",
            ref_prop!(),
            ",",
            xy_props!(),
            ",\"modifiers\":{\"type\":\"string\",\"description\":\"Optional held keys, e.g. \\\"shift\\\" or \\\"ctrl+shift\\\".\"},",
            tab_prop!(),
            "}}"
        ),
        run: tool_click,
    },
    Tool {
        name: "double_click",
        title: "Double-click",
        description: "Double-click an element (by ref) or a viewport point.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{",
            ref_prop!(),
            ",",
            xy_props!(),
            ",",
            tab_prop!(),
            "}}"
        ),
        run: tool_double_click,
    },
    Tool {
        name: "hover",
        title: "Hover",
        description: "Move the mouse pointer over an element (by ref) or a viewport point.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{",
            ref_prop!(),
            ",",
            xy_props!(),
            ",",
            tab_prop!(),
            "}}"
        ),
        run: tool_hover,
    },
    Tool {
        name: "type",
        title: "Type text",
        description: "Type text into the focused element, or click the element given by ref first. Newlines press Enter.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"},",
            ref_prop!(),
            ",",
            tab_prop!(),
            "},\"required\":[\"text\"]}"
        ),
        run: tool_type,
    },
    Tool {
        name: "key",
        title: "Press keys",
        description: "Press keys or shortcuts, space separated, e.g. \"Enter\", \"Tab\", \"ctrl+a\", \"shift+Tab\", \"ArrowDown ArrowDown Enter\", \"Escape\", \"PageDown\", \"F5\".",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"keys\":{\"type\":\"string\"},\"repeat\":{\"type\":\"integer\",\"description\":\"Repeat the whole sequence this many times (default 1).\"},",
            tab_prop!(),
            "},\"required\":[\"keys\"]}"
        ),
        run: tool_key,
    },
    Tool {
        name: "scroll",
        title: "Scroll",
        description: "Scroll with the mouse wheel at an element (by ref), a viewport point, or the centre of the viewport. With only a ref and no direction, scrolls that element into view.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"direction\":{\"type\":\"string\",\"enum\":[\"up\",\"down\",\"left\",\"right\"]},\"amount\":{\"type\":\"number\",\"description\":\"Wheel ticks of 100 px (default 3).\"},",
            ref_prop!(),
            ",",
            xy_props!(),
            ",",
            tab_prop!(),
            "}}"
        ),
        run: tool_scroll,
    },
    Tool {
        name: "form_input",
        title: "Set form value",
        description: "Set the value of an input, textarea, select (by option value or label), checkbox or radio (true/false), or contenteditable element, firing input and change events.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{",
            ref_prop!(),
            ",\"value\":{\"description\":\"New value: string, number, boolean, or an array of option values for a multiple select.\"},",
            tab_prop!(),
            "},\"required\":[\"ref\",\"value\"]}"
        ),
        run: tool_form_input,
    },
    Tool {
        name: "screenshot",
        title: "Screenshot",
        description: "Capture the visible viewport of a tab as a PNG. Image pixels are CSS viewport pixels, the same coordinates click, hover and scroll take.",
        schema: concat!("{\"type\":\"object\",\"properties\":{", tab_prop!(), "}}"),
        run: tool_screenshot,
    },
    Tool {
        name: "javascript",
        title: "Run JavaScript",
        description: "Evaluate JavaScript in the page's global scope and return the value of the last expression (objects as JSON). A returned Promise is awaited.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"code\":{\"type\":\"string\"},",
            tab_prop!(),
            "},\"required\":[\"code\"]}"
        ),
        run: tool_javascript,
    },
    Tool {
        name: "get_page_text",
        title: "Get page text",
        description: "Return the page's readable text (title, URL and body text) without markup.",
        schema: concat!(
            "{\"type\":\"object\",\"properties\":{\"max_chars\":{\"type\":\"integer\",\"description\":\"Truncate after this many characters (default 100000).\"},",
            tab_prop!(),
            "}}"
        ),
        run: tool_get_page_text,
    },
    Tool {
        name: "console",
        title: "Read console",
        description: "Return the tab's console messages since the last call: console.* output from the page and its workers, uncaught errors, unhandled rejections and failed resources.",
        schema: concat!("{\"type\":\"object\",\"properties\":{", tab_prop!(), "}}"),
        run: tool_console,
    },
    Tool {
        name: "wait",
        title: "Wait",
        description: "Wait for a number of milliseconds (at most 30000), e.g. for a page to update.",
        schema: "{\"type\":\"object\",\"properties\":{\"ms\":{\"type\":\"integer\",\"description\":\"Milliseconds to wait (default 1000).\"}}}",
        run: tool_wait,
    },
];

fn send_line(out: &mut impl Write, s: &str) {
    let _ = out.write_all(s.as_bytes());
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

fn send_result(out: &mut impl Write, id: &Value, result_json: &str) {
    send_line(
        out,
        &format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{result_json}}}",
            id.to_json()
        ),
    );
}

fn send_error(out: &mut impl Write, id: &Value, code: i32, message: &str) {
    send_line(
        out,
        &format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"error\":{{\"code\":{code},\"message\":{}}}}}",
            id.to_json(),
            quoted(message)
        ),
    );
}

fn pretty_client_name(info: Option<&Value>) -> Option<String> {
    let info = info?;
    if let Some(title) = info.str("title").filter(|t| !t.is_empty()) {
        return Some(title.to_owned());
    }
    let name = info.str("name").filter(|n| !n.is_empty())?;
    let mut out = String::new();
    let mut start = true;
    for c in name.chars() {
        if c == '-' || c == '_' {
            out.push(' ');
            start = true;
            continue;
        }
        if start {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
        start = false;
    }
    Some(out)
}

fn handle_initialize(m: &mut Mcp, out: &mut impl Write, id: &Value, params: Option<&Value>) {
    m.client_name = pretty_client_name(params.and_then(|p| p.get("clientInfo")));
    let asked = params.and_then(|p| p.str("protocolVersion"));
    let version = KNOWN_VERSIONS
        .iter()
        .find(|v| Some(**v) == asked)
        .copied()
        .unwrap_or(PROTOCOL_VERSION);
    let instructions = "These tools drive the Southstar browser window on the user's computer. Start with tabs_list or navigate, then read_page or find to get element refs (ref_N) and act on them with click, type, form_input, hover or scroll. Use screenshot to see the page; its pixel coordinates are what x/y arguments expect. Refs go stale after navigation: read the page again.";
    let result = format!(
        "{{\"protocolVersion\":{},\"capabilities\":{{\"tools\":{{\"listChanged\":false}}}},\"serverInfo\":{{\"name\":\"southstar\",\"title\":\"Southstar Browser\",\"version\":{}}},\"instructions\":{}}}",
        quoted(version),
        quoted(env!("CARGO_PKG_VERSION")),
        quoted(instructions)
    );
    send_result(out, id, &result);
}

fn handle_tools_list(out: &mut impl Write, id: &Value) {
    let mut s = String::from("{\"tools\":[");
    for (i, tool) in TOOLS.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "{{\"name\":{},\"title\":{},\"description\":{},\"inputSchema\":{}}}",
            quoted(tool.name),
            quoted(tool.title),
            quoted(tool.description),
            tool.schema
        );
    }
    s.push_str("]}");
    send_result(out, id, &s);
}

fn handle_tools_call(m: &mut Mcp, out: &mut impl Write, id: &Value, params: Option<&Value>) {
    let name = params.and_then(|p| p.str("name"));
    let Some(tool) = TOOLS.iter().find(|t| Some(t.name) == name) else {
        send_error(out, id, -32602, "unknown tool");
        return;
    };
    let empty = Value::Obj(Vec::new());
    let args = params
        .and_then(|p| p.get("arguments"))
        .filter(|a| a.is_object())
        .unwrap_or(&empty);
    let mut res = ToolResult::default();
    (tool.run)(m, args, &mut res);
    let mut s = String::from("{\"content\":[");
    if let Some(image) = &res.image {
        let _ = write!(
            s,
            "{{\"type\":\"image\",\"mimeType\":\"image/png\",\"data\":{}}},",
            quoted(image)
        );
    }
    let text = if res.text.is_empty() {
        "Done."
    } else {
        &res.text
    };
    let _ = write!(
        s,
        "{{\"type\":\"text\",\"text\":{}}}],\"isError\":{}}}",
        quoted(text),
        res.error
    );
    send_result(out, id, &s);
}

fn handle_message(m: &mut Mcp, out: &mut impl Write, line: &[u8]) {
    let Some(msg) = json::parse(line).filter(Value::is_object) else {
        send_error(out, &Value::Null, -32700, "parse error");
        return;
    };
    let (Some(method), Some(id)) = (msg.str("method"), msg.get("id")) else {
        return;
    };
    let params = msg.get("params");
    match method {
        "initialize" => handle_initialize(m, out, id, params),
        "ping" => send_result(out, id, "{}"),
        "tools/list" => handle_tools_list(out, id),
        "tools/call" => handle_tools_call(m, out, id, params),
        "resources/list" => send_result(out, id, "{\"resources\":[]}"),
        "prompts/list" => send_result(out, id, "{\"prompts\":[]}"),
        _ => send_error(out, id, -32601, "method not found"),
    }
}

pub fn run(self_exe: Option<PathBuf>) -> i32 {
    let mut m = Mcp {
        self_exe,
        client_name: None,
        channel: None,
        next_id: 0,
    };
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    while let Ok(Some(line)) = read_line(&mut input, MAX_LINE) {
        if !line.is_empty() {
            handle_message(&mut m, &mut out, &line);
        }
    }
    m.drop_channel();
    0
}
