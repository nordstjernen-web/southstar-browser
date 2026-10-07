//! Southstar — browsing history: the SQLite visits table and the about:history page built from it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::ffi::CStr;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use ffi::{Db, LocalTime};

const MAX_ROWS: i32 = 10000;
const PAGE_ROWS: i32 = 200;
const APP_DIR_NAME: &CStr = c"southstar";

const SCHEMA: [&CStr; 4] = [
    c"PRAGMA journal_mode=WAL",
    c"PRAGMA synchronous=NORMAL",
    c"CREATE TABLE IF NOT EXISTS visits(url TEXT PRIMARY KEY,title TEXT,visit_count INTEGER NOT NULL DEFAULT 1,last_visit INTEGER NOT NULL)",
    c"CREATE INDEX IF NOT EXISTS idx_visits_last ON visits(last_visit)",
];
const PRUNE: &CStr =
    c"DELETE FROM visits WHERE url NOT IN (SELECT url FROM visits ORDER BY last_visit DESC LIMIT ?)";
const RECORD: &CStr = c"INSERT INTO visits(url,title,visit_count,last_visit) VALUES(?,?,1,?) ON CONFLICT(url) DO UPDATE SET visit_count=visit_count+1,last_visit=excluded.last_visit,title=COALESCE(NULLIF(excluded.title,''),title)";
const CLEAR: &CStr = c"DELETE FROM visits";
const RECENT: &CStr = c"SELECT url,title,last_visit FROM visits ORDER BY last_visit DESC LIMIT ?";

const PAGE_HEAD: &str = concat!(
    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
    "<meta name=\"color-scheme\" content=\"light dark\">",
    "<title>History</title>",
);
const PAGE_STYLE: &str = concat!(
    ".wrap{max-width:860px;margin:0 auto;padding:28px 24px 56px}\n",
    ".top{display:flex;align-items:center;justify-content:space-between;",
    "gap:16px;margin:18px 0 24px}\n",
    "h1{margin:0;font-size:28px;letter-spacing:-.02em}\n",
    ".filter{flex:0 1 320px;display:flex;align-items:center;gap:10px;",
    "height:42px;padding:0 16px;border-radius:999px;background:var(--card);",
    "border:1px solid var(--line);box-shadow:var(--shadow);",
    "color:var(--faint)}\n",
    ".filter:focus-within{border-color:var(--accent);",
    "box-shadow:0 0 0 3px var(--accent-soft)}\n",
    ".filter input{flex:1 1 auto;min-width:0;border:0;outline:0;",
    "background:transparent;color:var(--text);font:inherit;font-size:14px}\n",
    ".day{margin:0 0 18px}\n",
    ".day h2{margin:0 0 8px 6px;font-size:13px;font-weight:700;",
    "letter-spacing:.06em;text-transform:uppercase;color:var(--faint)}\n",
    ".day ul{list-style:none;margin:0;padding:6px;}\n",
    ".day li a{display:flex;align-items:center;gap:14px;padding:9px 12px;",
    "border-radius:12px;color:var(--text)}\n",
    ".day li a:hover{background:var(--field);text-decoration:none}\n",
    ".av{flex:0 0 auto;display:flex;align-items:center;",
    "justify-content:center;width:32px;height:32px;border-radius:10px;",
    "background:var(--accent-soft);color:var(--accent);font-weight:700;",
    "font-size:14px;text-transform:uppercase}\n",
    ".tx{flex:1 1 auto;min-width:0;display:flex;flex-direction:column}\n",
    ".t{font-size:14.5px;font-weight:600;white-space:nowrap;overflow:hidden;",
    "text-overflow:ellipsis}\n",
    ".u{font-size:12.5px;color:var(--muted);white-space:nowrap;",
    "overflow:hidden;text-overflow:ellipsis}\n",
    ".tm{flex:0 0 auto;font-size:12.5px;color:var(--faint);",
    "font-variant-numeric:tabular-nums}\n",
    ".empty{padding:48px 24px;text-align:center;color:var(--muted)}\n",
    ".empty b{display:block;margin-bottom:4px;font-size:16px;",
    "color:var(--text)}\n",
    "[hidden]{display:none}\n",
    "</style>",
);
const PAGE_TOP: &str = concat!(
    "</head><body><main class=\"wrap\">",
    "<a class=\"crumb\" href=\"about:start\">\u{2190} New Tab</a>",
    "<div class=\"top\"><h1>History</h1>",
    "<label class=\"filter\">",
    "<svg width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"none\"",
    " stroke=\"currentColor\" stroke-width=\"1.6\"",
    " stroke-linecap=\"round\"><circle cx=\"7\" cy=\"7\" r=\"4.6\"/>",
    "<path d=\"M10.4 10.4 14 14\"/></svg>",
    "<input id=\"hq\" type=\"search\" placeholder=\"Search history\"",
    " aria-label=\"Search history\" autocomplete=\"off\"></label></div>",
);
const PAGE_EMPTY: &str = concat!(
    "<div class=\"card empty\"><b>No history yet</b>",
    "Pages you visit will show up here.</div>",
);
const PAGE_SCRIPT: &str = concat!(
    "<script>\n",
    "var q=document.getElementById('hq');\n",
    "if(q)q.addEventListener('input',function(){\n",
    " var v=q.value.toLowerCase();\n",
    " var days=document.querySelectorAll('.day');\n",
    " for(var i=0;i<days.length;i++){var any=false;\n",
    "  var rows=days[i].querySelectorAll('li');\n",
    "  for(var j=0;j<rows.length;j++){\n",
    "   var hit=!v||rows[j].textContent.toLowerCase().indexOf(v)>=0;\n",
    "   rows[j].hidden=!hit;if(hit)any=true;}\n",
    "  days[i].hidden=!any;}});\n",
    "</script>",
);

struct History {
    db: Option<Db>,
    disabled: bool,
}

static HISTORY: Mutex<History> = Mutex::new(History {
    db: None,
    disabled: false,
});

fn lock() -> MutexGuard<'static, History> {
    HISTORY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn exec(db: &Db, sql: &CStr) -> bool {
    match db.exec(sql) {
        Ok(()) => true,
        Err(message) => {
            ffi::warn(&[b"history: sqlite exec failed: ".as_slice(), &message].concat());
            false
        }
    }
}

pub fn is_recordable(url: &[u8]) -> bool {
    (url.starts_with(b"http://") || url.starts_with(b"https://"))
        && !url.iter().any(|&c| c < 0x20 || c == 0x7f)
}

fn prune(db: &Db) {
    if let Some(st) = db.prepare(PRUNE) {
        st.bind_int(1, MAX_ROWS);
        st.step();
    }
}

fn init() {
    let mut history = lock();
    if history.db.is_some() || history.disabled {
        return;
    }
    let dir = ffi::build_filename(&ffi::user_data_dir(), APP_DIR_NAME);
    ffi::mkdir_with_parents(&dir, 0o700);
    ffi::chmod(&dir, 0o700);
    let path = ffi::build_filename(&dir, c"history.sqlite");
    let db = match Db::open(&path) {
        Ok(db) => db,
        Err(message) => {
            ffi::warn(
                &[
                    b"history: could not open ".as_slice(),
                    path.to_bytes(),
                    b": ",
                    &message,
                ]
                .concat(),
            );
            history.disabled = true;
            return;
        }
    };
    db.harden();
    db.busy_timeout(2500);
    if !SCHEMA.iter().all(|sql| exec(&db, sql)) {
        history.disabled = true;
        return;
    }
    ffi::chmod(&path, 0o600);
    prune(&db);
    history.db = Some(db);
}

fn shutdown() {
    lock().db = None;
}

fn record(url: Option<&CStr>, title: Option<&CStr>) {
    let Some(url) = url.filter(|url| is_recordable(url.to_bytes())) else {
        return;
    };
    let history = lock();
    let Some(st) = history.db.as_ref().and_then(|db| db.prepare(RECORD)) else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    st.bind_text(1, Some(url));
    st.bind_text(2, title.filter(|title| !title.is_empty()));
    st.bind_int64(3, now);
    st.step();
}

fn clear() {
    if let Some(db) = lock().db.as_ref() {
        exec(db, CLEAR);
    }
}

fn day_label(when: &LocalTime, now: &LocalTime) -> Vec<u8> {
    let days = now.day_of_year() - when.day_of_year();
    let same_year = now.year() == when.year();
    match (same_year, days) {
        (true, 0) => b"Today".to_vec(),
        (true, 1) => b"Yesterday".to_vec(),
        (true, _) => when.format(c"%A, %e %B").unwrap_or_default(),
        (false, _) => when.format(c"%A, %e %B %Y").unwrap_or_default(),
    }
}

fn host(url: &[u8]) -> Vec<u8> {
    let host = ffi::uri_host(url).filter(|host| !host.is_empty());
    let out = ffi::utf8_make_valid(host.as_deref().unwrap_or(url));
    match out.strip_prefix(b"www.") {
        Some(trimmed) => trimmed.to_vec(),
        None => out,
    }
}

fn initial(host: &[u8]) -> Vec<u8> {
    match ffi::first_char(host).filter(|&c| ffi::unichar_isalnum(c)) {
        Some(c) => ffi::unichar_to_utf8(c),
        None => "\u{2022}".as_bytes().to_vec(),
    }
}

fn switch_day(page: &mut Vec<u8>, open_day: &mut Option<Vec<u8>>, day: &[u8]) {
    if open_day.as_deref() == Some(day) {
        return;
    }
    if open_day.is_some() {
        page.extend_from_slice(b"</ul></section>");
    }
    page.extend_from_slice(b"<section class=\"day\"><h2>");
    page.extend_from_slice(&ffi::markup_escape(day));
    page.extend_from_slice(b"</h2><ul class=\"card\">");
    *open_day = Some(day.to_vec());
}

fn append_row(
    page: &mut Vec<u8>,
    raw_url: &[u8],
    raw_title: Option<&[u8]>,
    when: Option<&LocalTime>,
) {
    let url = ffi::utf8_make_valid(raw_url);
    let title = raw_title
        .filter(|title| !title.is_empty())
        .map(ffi::utf8_make_valid);
    let host = host(&url);
    let initial = initial(&host);
    let clock = when.and_then(|when| when.format(c"%H:%M"));
    for piece in [
        b"<li><a href=\"".as_slice(),
        &ffi::markup_escape(&url),
        b"\"><span class=\"av\">",
        &ffi::markup_escape(&initial),
        b"</span><span class=\"tx\"><span class=\"t\">",
        &ffi::markup_escape(title.as_deref().unwrap_or(&url)),
        b"</span><span class=\"u\">",
        &ffi::markup_escape(&host),
        b"</span></span><span class=\"tm\">",
        clock.as_deref().unwrap_or_default(),
        b"</span></a></li>",
    ] {
        page.extend_from_slice(piece);
    }
}

fn append_visits(page: &mut Vec<u8>) -> bool {
    let mut have = false;
    let mut open_day = None;
    let now = LocalTime::now();
    {
        let history = lock();
        if let Some(st) = history.db.as_ref().and_then(|db| db.prepare(RECENT)) {
            st.bind_int(1, PAGE_ROWS);
            while st.step() {
                let Some(url) = st.column_text(0) else {
                    continue;
                };
                let title = st.column_text(1);
                have = true;
                let when = LocalTime::from_unix(st.column_int64(2));
                let day = match &when {
                    Some(when) => day_label(when, &now),
                    None => b"Earlier".to_vec(),
                };
                switch_day(page, &mut open_day, &day);
                append_row(page, &url, title.as_deref(), when.as_ref());
            }
        }
    }
    if open_day.is_some() {
        page.extend_from_slice(b"</ul></section>");
    }
    have
}

fn html_page() -> Vec<u8> {
    let mut page = Vec::new();
    page.extend_from_slice(PAGE_HEAD.as_bytes());
    page.extend_from_slice(b"<style>");
    page.extend_from_slice(southstar_about_style::base_css().as_bytes());
    page.extend_from_slice(PAGE_STYLE.as_bytes());
    page.extend_from_slice(PAGE_TOP.as_bytes());
    if !append_visits(&mut page) {
        page.extend_from_slice(PAGE_EMPTY.as_bytes());
    }
    page.extend_from_slice(b"</main>");
    page.extend_from_slice(PAGE_SCRIPT.as_bytes());
    page.extend_from_slice(b"</body></html>");
    page
}
