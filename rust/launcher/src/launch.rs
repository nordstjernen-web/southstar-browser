//! Southstar — the launcher's rules: which arguments want the console, where the browser lives, how its command line is quoted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const APP_DIR: &str = "app";
pub const BROWSER_EXE: &str = "southstar-ui.exe";
pub const EXIT_MISSING_RUNTIME: i32 = 127;
pub const STATUS_DLL_NOT_FOUND: u32 = 0xC000_0135;
pub const STATUS_ENTRYPOINT_NOT_FOUND: u32 = 0xC000_0139;

pub const EXTRACT_NEEDED: &str = "Southstar must be extracted before it can run.\n\nIn File Explorer, right-click the ZIP, choose Extract All, then run southstar.exe from the extracted southstar-win64 folder.";
pub const RUNTIME_MISSING: &str = "Southstar could not find its bundled runtime files.\n\nExtract the whole southstar-win64 folder and keep southstar.exe and the app folder together.";

pub fn start_error(error: u32) -> String {
    format!("Southstar could not start.\n\nWindows error: {error}")
}

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn starts_with(s: &[u16], prefix: &str) -> bool {
    let p = wide(prefix);
    s.len() >= p.len() && s[..p.len()] == p[..]
}

fn equals(s: &[u16], other: &str) -> bool {
    s == &wide(other)[..]
}

pub fn args_need_console(args: &[Vec<u16>]) -> bool {
    args.iter().skip(1).any(|a| {
        equals(a, "--headless")
            || equals(a, "--print-config")
            || [
                "--dump=",
                "--url=",
                "--viewport=",
                "--eval=",
                "--inspect=",
                "--inspect-at=",
                "--settle-ms=",
            ]
            .iter()
            .any(|p| starts_with(a, p))
    })
}

const BACKSLASH: u16 = b'\\' as u16;
const SLASH: u16 = b'/' as u16;
const COLON: u16 = b':' as u16;
const QUOTE: u16 = b'"' as u16;
const SPACE: u16 = b' ' as u16;

pub fn dirname(path: &[u16]) -> Vec<u16> {
    let back = path.iter().rposition(|&c| c == BACKSLASH);
    let fwd = path.iter().rposition(|&c| c == SLASH);
    let slash = match (back, fwd) {
        (Some(b), Some(f)) => Some(b.max(f)),
        (b, f) => b.or(f),
    };
    match slash {
        None => wide("."),
        Some(0) => path[..1].to_vec(),
        Some(2) if path.get(1) == Some(&COLON) => path[..3].to_vec(),
        Some(i) => path[..i].to_vec(),
    }
}

pub fn join_path(dir: &[u16], name: &str) -> Vec<u16> {
    let mut out = dir.to_vec();
    if out.last().is_some_and(|&c| c != BACKSLASH && c != SLASH) {
        out.push(BACKSLASH);
    }
    out.extend(wide(name));
    out
}

fn push_quoted(out: &mut Vec<u16>, arg: &[u16]) {
    out.push(QUOTE);
    let mut slashes = 0;
    for &c in arg {
        if c == BACKSLASH {
            slashes += 1;
            continue;
        }
        if c == QUOTE {
            out.extend(core::iter::repeat_n(BACKSLASH, slashes * 2 + 1));
            out.push(c);
            slashes = 0;
            continue;
        }
        out.extend(core::iter::repeat_n(BACKSLASH, slashes));
        slashes = 0;
        out.push(c);
    }
    out.extend(core::iter::repeat_n(BACKSLASH, slashes * 2));
    out.push(QUOTE);
}

pub fn command_line(browser_path: &[u16], args: &[Vec<u16>]) -> Vec<u16> {
    let mut out = Vec::new();
    push_quoted(&mut out, browser_path);
    for arg in args.iter().skip(1) {
        out.push(SPACE);
        push_quoted(&mut out, arg);
    }
    out
}
