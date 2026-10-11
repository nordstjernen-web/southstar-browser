//! Southstar — the task manager's numbers: per-process CPU percentages between refreshes and the column text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

pub const DASH: &str = "—";

struct Sample {
    base_cpu: f64,
    base_us: i64,
    pct: f64,
    tick: u32,
}

pub struct CpuHistory {
    samples: HashMap<i32, Sample>,
    pub now_us: i64,
    pub tick: u32,
    pub ncpu: i32,
}

impl CpuHistory {
    pub fn new(ncpu: i32) -> CpuHistory {
        CpuHistory {
            samples: HashMap::new(),
            now_us: 0,
            tick: 0,
            ncpu,
        }
    }

    pub fn begin(&mut self, now_us: i64) {
        self.tick = self.tick.wrapping_add(1);
        self.now_us = now_us;
    }

    pub fn pct(&mut self, pid: i32, cpu_now: f64) -> f64 {
        if pid <= 0 || cpu_now < 0.0 {
            return -1.0;
        }
        let (now_us, tick, ncpu) = (self.now_us, self.tick, self.ncpu);
        let Some(h) = self.samples.get_mut(&pid) else {
            self.samples.insert(
                pid,
                Sample {
                    base_cpu: cpu_now,
                    base_us: now_us,
                    pct: -1.0,
                    tick,
                },
            );
            return -1.0;
        };
        if h.tick == tick {
            return h.pct;
        }
        let dt = (now_us - h.base_us) as f64 / 1e6;
        let mut pct = -1.0;
        if dt >= 0.1 && cpu_now >= h.base_cpu {
            pct = (cpu_now - h.base_cpu) / dt * 100.0;
            let cap = if ncpu > 0 {
                f64::from(ncpu) * 100.0
            } else {
                100.0
            };
            if pct > cap {
                pct = cap;
            }
            if pct < 0.0 {
                pct = 0.0;
            }
        }
        h.base_cpu = cpu_now;
        h.base_us = now_us;
        h.pct = pct;
        h.tick = tick;
        pct
    }

    pub fn prune(&mut self) {
        let tick = self.tick;
        self.samples.retain(|_, h| h.tick == tick);
    }
}

pub fn count(n: i32) -> String {
    if n > 0 { n.to_string() } else { DASH.into() }
}

pub fn threads(n: i32) -> String {
    if n >= 0 { n.to_string() } else { DASH.into() }
}

pub fn memory(rss_kb: impl Into<i64>) -> String {
    let rss_kb: i64 = rss_kb.into();
    if rss_kb >= 0 {
        format!("{:.1} MB", rss_kb as f64 / 1024.0)
    } else {
        DASH.into()
    }
}

pub fn cpu_time(cpu: f64) -> String {
    if cpu < 0.0 {
        DASH.into()
    } else if cpu >= 60.0 {
        let minutes = cpu as i32 / 60;
        let seconds = cpu - f64::from((cpu / 60.0) as i32 * 60);
        format!("{minutes}:{seconds:04.1}")
    } else {
        format!("{cpu:.1} s")
    }
}

pub fn percent(pct: f64) -> String {
    if pct < 0.0 {
        DASH.into()
    } else {
        format!("{pct:.1} %")
    }
}
