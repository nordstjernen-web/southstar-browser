//! Southstar — the supervisor that restarts the browser on crash or hang: its arguments, restart policy and hang budget.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::c_int;
use std::time::{Duration, Instant};

pub const FLAG: &[u8] = b"--watchdog";
pub const NO_FLAG: &[u8] = b"--no-watchdog";
pub const CHILD_FLAG: &[u8] = b"--watchdog-child";
pub const SESSION_PREFIX: &[u8] = b"--watchdog-session=";

pub const BEAT_SECS: u32 = 2;
pub const CHECK_INTERVAL: Duration = Duration::from_secs(1);
pub const HANG_MIN_SECS: c_int = 60;
pub const BACKOFF_MS: u32 = 1000;
pub const BURST_MAX: c_int = 5;
pub const BURST_SECS: u64 = 60;
pub const STOP_GRACE_SECS: u32 = 3;
pub const HANG_EXIT: c_int = 70;

const SIGHUP: c_int = 1;
const SIGINT: c_int = 2;
const SIGQUIT: c_int = 3;
const SIGKILL: c_int = 9;
const SIGTERM: c_int = 15;

pub fn session_arg<'a>(args: impl IntoIterator<Item = &'a [u8]>) -> Option<&'a [u8]> {
    args.into_iter()
        .find_map(|arg| arg.strip_prefix(SESSION_PREFIX))
}

pub fn is_child<'a>(args: impl IntoIterator<Item = &'a [u8]>) -> bool {
    args.into_iter().any(|arg| arg == CHILD_FLAG)
}

fn is_oneshot(arg: &[u8]) -> bool {
    arg == b"--headless"
        || arg == b"--print-config"
        || [&b"--dump="[..], b"--eval=", b"--inspect=", b"--inspect-at="]
            .iter()
            .any(|prefix| arg.starts_with(prefix))
}

pub fn should_supervise<'a>(
    args: impl IntoIterator<Item = &'a [u8]>,
    enabled_by_default: bool,
) -> bool {
    let mut forced = false;
    for arg in args {
        if arg == CHILD_FLAG || arg == NO_FLAG || is_oneshot(arg) {
            return false;
        }
        forced |= arg == FLAG;
    }
    forced || enabled_by_default
}

pub fn child_args<'a>(
    self_exe: &[u8],
    args: impl IntoIterator<Item = &'a [u8]>,
    session_path: &[u8],
) -> Vec<Vec<u8>> {
    let mut child = vec![self_exe.to_vec()];
    child.extend(
        args.into_iter()
            .filter(|arg| {
                *arg != FLAG
                    && *arg != NO_FLAG
                    && *arg != CHILD_FLAG
                    && !arg.starts_with(SESSION_PREFIX)
            })
            .map(<[u8]>::to_vec),
    );
    child.push(CHILD_FLAG.to_vec());
    child.push([SESSION_PREFIX, session_path].concat());
    child
}

pub fn hang_seconds(js_budget_ms: c_int) -> c_int {
    (js_budget_ms / 1000 + HANG_MIN_SECS).max(HANG_MIN_SECS)
}

pub struct HangMonitor {
    last_beat: i32,
    last_change: Instant,
}

impl HangMonitor {
    pub fn new(beat: i32, now: Instant) -> HangMonitor {
        HangMonitor {
            last_beat: beat,
            last_change: now,
        }
    }

    pub fn hung(&mut self, beat: i32, now: Instant, hang_secs: c_int) -> bool {
        if beat != self.last_beat {
            self.last_beat = beat;
            self.last_change = now;
            return false;
        }
        now.duration_since(self.last_change)
            > Duration::from_secs(u64::try_from(hang_secs).unwrap_or(0))
    }
}

pub enum Restart {
    Attempt(c_int),
    GiveUp(c_int),
}

pub struct Burst {
    count: c_int,
    start: Instant,
}

impl Burst {
    pub fn new(now: Instant) -> Burst {
        Burst {
            count: 0,
            start: now,
        }
    }

    pub fn record(&mut self, now: Instant) -> Restart {
        if now.duration_since(self.start) > Duration::from_secs(BURST_SECS) {
            self.start = now;
            self.count = 0;
        }
        self.count += 1;
        if self.count > BURST_MAX {
            Restart::GiveUp(self.count)
        } else {
            Restart::Attempt(self.count)
        }
    }
}

pub enum ChildExit {
    Clean,
    StoppedBySignal(c_int),
    KilledBySignal(c_int),
    Failed(c_int),
}

pub fn classify_exit(status: c_int) -> ChildExit {
    if cfg!(windows) {
        return if status == 0 {
            ChildExit::Clean
        } else {
            ChildExit::Failed(status)
        };
    }
    let signal = status & 0x7f;
    if signal == 0 {
        let code = (status >> 8) & 0xff;
        return if code == 0 {
            ChildExit::Clean
        } else {
            ChildExit::Failed(code)
        };
    }
    if signal == 0x7f {
        return ChildExit::Failed((status >> 8) & 0xff);
    }
    if matches!(signal, SIGTERM | SIGINT | SIGHUP | SIGQUIT | SIGKILL) {
        ChildExit::StoppedBySignal(signal)
    } else {
        ChildExit::KilledBySignal(signal)
    }
}

fn quote_windows_arg(arg: &str, out: &mut Vec<u16>) {
    const BACKSLASH: u16 = b'\\' as u16;
    const QUOTE: u16 = b'"' as u16;
    out.push(QUOTE);
    let mut slashes = 0;
    for unit in arg.encode_utf16() {
        if unit == BACKSLASH {
            slashes += 1;
            continue;
        }
        let escaped = if unit == QUOTE {
            slashes * 2 + 1
        } else {
            slashes
        };
        out.extend(core::iter::repeat_n(BACKSLASH, escaped));
        out.push(unit);
        slashes = 0;
    }
    out.extend(core::iter::repeat_n(BACKSLASH, slashes * 2));
    out.push(QUOTE);
}

pub fn windows_command_line(args: &[Vec<u8>]) -> Option<Vec<u16>> {
    let mut line = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            line.push(u16::from(b' '));
        }
        quote_windows_arg(core::str::from_utf8(arg).ok()?, &mut line);
    }
    Some(line)
}
