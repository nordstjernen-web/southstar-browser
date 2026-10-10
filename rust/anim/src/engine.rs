//! Southstar — the animation engine: transition channels and keyframe runs per element, observing computed styles, ticking, sampling into styles, events and the Web Animations controls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;
use core::ffi::CStr;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use southstar_style::{StyleRef, display_of};

use crate::ffi::css::{
    self, AnimList, Borrowed, Decls, Entry, KeyframeSource, Keyframes, NodePtr, PROP_COUNT,
    ResolvedKeyframes, RetainedStyle, StyleMut, StylesTable, Target, Val,
};
use crate::props;
use crate::timing::Timing;

const MAX_ACTIVE: i32 = 256;
pub const SCRIPT_BASE: i32 = 1000;

pub const DIR_NORMAL: i32 = 0;
pub const DIR_REVERSE: i32 = 1;
pub const DIR_ALTERNATE: i32 = 2;
pub const DIR_ALTERNATE_REVERSE: i32 = 3;

pub const FILL_NONE: i32 = 0;
pub const FILL_FORWARDS: i32 = 1;
pub const FILL_BACKWARDS: i32 = 2;
pub const FILL_BOTH: i32 = 3;

pub const TARGET_COLOR: i32 = 4;
pub const TARGET_BG_COLOR: i32 = 5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Before,
    Active,
    After,
}

fn gclamp(x: f64, lo: f64, hi: f64) -> f64 {
    if x > hi {
        hi
    } else if x < lo {
        lo
    } else {
        x
    }
}

fn gmax(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn gmin(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

pub struct Event {
    pub node: NodePtr,
    pub kind: &'static CStr,
    pub name: Vec<u8>,
    pub elapsed_ms: f64,
}

struct Chan {
    prop: i32,
    last: Option<Val>,
    from: Option<Val>,
    to: Option<Val>,
    current: Option<Val>,
    has_last: bool,
    last_currentcolor: bool,
    active: bool,
    cancelled: bool,
    discrete: bool,
    pending: bool,
    phase: Phase,
    started: bool,
    paused: bool,
    finished: bool,
    start_us: f64,
    duration_ms: f64,
    delay_ms: f64,
    paused_elapsed_ms: f64,
    timing: Timing,
    generation: u32,
}

impl Chan {
    fn new(prop: i32) -> Chan {
        Chan {
            prop,
            last: None,
            from: None,
            to: None,
            current: None,
            has_last: false,
            last_currentcolor: false,
            active: false,
            cancelled: false,
            discrete: false,
            pending: false,
            phase: Phase::Idle,
            started: false,
            paused: false,
            finished: false,
            start_us: 0.0,
            duration_ms: 0.0,
            delay_ms: 0.0,
            paused_elapsed_ms: 0.0,
            timing: Timing::default(),
            generation: 0,
        }
    }

    fn elapsed_ms(&self, now_us: i64) -> f64 {
        if self.paused {
            return self.paused_elapsed_ms;
        }
        if self.pending {
            return -self.delay_ms;
        }
        (now_us as f64 - self.start_us) / 1000.0 - self.delay_ms
    }

    fn phase_at(&self, elapsed: f64) -> Phase {
        if elapsed < 0.0 {
            Phase::Before
        } else if elapsed >= self.duration_ms {
            Phase::After
        } else {
            Phase::Active
        }
    }
}

struct KfStop {
    pct: f64,
    decls: Option<Decls>,
}

impl KfStop {
    fn value(&self, prop: i32) -> Borrowed {
        self.decls
            .as_ref()
            .map_or(Borrowed::NULL, |d| d.value_of(prop))
    }

    fn timing(&self) -> Option<Timing> {
        let keyword = self
            .value(props::known().animation_timing_function)
            .keyword()?;
        let mut timing = Timing::default();
        timing.parse_into(keyword).then_some(timing)
    }
}

struct Partial {
    from: Option<Val>,
    to: Option<Val>,
    t: f64,
}

struct Run {
    index: i32,
    is_script: bool,
    name: Option<Vec<u8>>,
    cancelled_name: Option<Vec<u8>>,
    active: bool,
    paused: bool,
    css_paused: bool,
    api_override: bool,
    started: bool,
    finished: bool,
    pending: bool,
    phase: Phase,
    iters_emitted: i32,
    elapsed_base_ms: f64,
    kf: Option<ResolvedKeyframes>,
    stops: Option<Vec<KfStop>>,
    start_us: f64,
    duration_ms: f64,
    delay_ms: f64,
    iterations: f64,
    direction: i32,
    fill: i32,
    timing: Timing,
    generation: u32,
    values: BTreeMap<i32, Val>,
    partials: BTreeMap<i32, Partial>,
}

impl Run {
    fn new(index: i32, is_script: bool) -> Run {
        Run {
            index,
            is_script,
            name: None,
            cancelled_name: None,
            active: false,
            paused: false,
            css_paused: false,
            api_override: false,
            started: false,
            finished: false,
            pending: false,
            phase: Phase::Idle,
            iters_emitted: 0,
            elapsed_base_ms: 0.0,
            kf: None,
            stops: None,
            start_us: 0.0,
            duration_ms: 0.0,
            delay_ms: 0.0,
            iterations: 0.0,
            direction: DIR_NORMAL,
            fill: FILL_NONE,
            timing: Timing::default(),
            generation: 0,
            values: BTreeMap::new(),
            partials: BTreeMap::new(),
        }
    }

    fn elapsed_ms(&self, now_us: i64) -> f64 {
        if self.paused {
            return self.elapsed_base_ms;
        }
        if self.pending {
            return -self.delay_ms;
        }
        (now_us as f64 - self.start_us) / 1000.0 - self.delay_ms
    }

    fn set_paused(&mut self, paused: bool, now_us: i64) {
        if paused && !self.paused {
            self.elapsed_base_ms = self.elapsed_ms(now_us);
            self.paused = true;
        } else if !paused && self.paused {
            self.start_us = now_us as f64 - (self.elapsed_base_ms + self.delay_ms) * 1000.0;
            self.paused = false;
        }
    }

    fn active_ms(&self) -> f64 {
        if !self.iterations.is_finite() {
            return f64::INFINITY;
        }
        self.duration_ms * self.iterations
    }

    fn active_time(&self, elapsed: f64) -> f64 {
        let active = self.active_ms();
        if elapsed < 0.0 {
            return 0.0;
        }
        if active.is_finite() && elapsed > active {
            return active;
        }
        elapsed
    }

    fn last_iteration(&self) -> i32 {
        if !self.iterations.is_finite() {
            return 0;
        }
        (self.iterations.ceil() as i32 - 1).max(0)
    }

    fn phase_at(&self, elapsed: f64) -> Phase {
        let active = self.active_ms();
        if elapsed < 0.0 {
            Phase::Before
        } else if active.is_finite() && elapsed >= active {
            Phase::After
        } else {
            Phase::Active
        }
    }

    fn configure(&mut self, e: Entry<'_>) {
        self.duration_ms = e.duration_ms();
        self.delay_ms = e.delay_ms();
        self.iterations = e.iterations();
        self.direction = e.direction();
        self.fill = e.fill();
        self.timing = e.timing();
    }

    fn clear_samples(&mut self) {
        self.values.clear();
        self.partials.clear();
    }

    fn in_effect(&self) -> bool {
        self.name.is_some()
            && (self.active
                || (self.finished && (self.fill == FILL_FORWARDS || self.fill == FILL_BOTH)))
    }

    fn sample_at(&mut self, progress: f64) {
        let shaped = if self.is_script {
            self.timing.apply(progress)
        } else {
            progress
        };
        let pct = shaped * 100.0;
        self.clear_samples();
        let Some(stops) = &self.stops else {
            return;
        };
        let mut seen = [false; PROP_COUNT];
        for stop in stops {
            let Some(decls) = &stop.decls else {
                continue;
            };
            for prop in decls.props() {
                let Some(slot) = usize::try_from(prop).ok().and_then(|i| seen.get_mut(i)) else {
                    continue;
                };
                if *slot {
                    continue;
                }
                *slot = true;
                if !props::animatable(prop) {
                    continue;
                }
                let (mut prev, mut next) = (None, None);
                for (k, c) in stops.iter().enumerate() {
                    if c.value(prop).is_null() {
                        continue;
                    }
                    if c.pct <= pct {
                        prev = Some(k);
                    }
                    if c.pct > pct && next.is_none() {
                        next = Some(k);
                    }
                }
                let prev = prev.map(|k| &stops[k]);
                let next = next.map(|k| &stops[k]);
                let mut out = None;
                match (prev, next) {
                    (Some(p), Some(n)) if !core::ptr::eq(p, n) => {
                        let range = n.pct - p.pct;
                        let mut t = if range > 0.0 {
                            (pct - p.pct) / range
                        } else {
                            0.0
                        };
                        if !self.is_script {
                            t = p.timing().unwrap_or(self.timing).apply(t);
                        }
                        out = Val::interpolate(p.value(prop), n.value(prop), t).or_else(|| {
                            Val::retain(if t < 0.5 {
                                p.value(prop)
                            } else {
                                n.value(prop)
                            })
                        });
                    }
                    (Some(p), _) if p.pct >= 100.0 - 1e-9 => out = Val::retain(p.value(prop)),
                    (_, Some(n)) if n.pct <= 1e-9 => out = Val::retain(n.value(prop)),
                    (None, None) => {}
                    _ => {
                        let lo = prev.map_or(0.0, |p| p.pct);
                        let hi = next.map_or(100.0, |n| n.pct);
                        let mut t = if hi > lo { (pct - lo) / (hi - lo) } else { 0.0 };
                        if !self.is_script {
                            let timing = match prev {
                                Some(p) => p.timing().unwrap_or(self.timing),
                                None => self.timing,
                            };
                            t = timing.apply(t);
                        }
                        self.partials.insert(
                            prop,
                            Partial {
                                from: prev.and_then(|p| Val::retain(p.value(prop))),
                                to: next.and_then(|n| Val::retain(n.value(prop))),
                                t,
                            },
                        );
                    }
                }
                if let Some(out) = out {
                    self.values.insert(prop, out);
                }
            }
        }
    }
}

fn directed_progress(iter: i32, raw: f64, direction: i32) -> f64 {
    let reverse = match direction {
        DIR_REVERSE => true,
        DIR_ALTERNATE => iter & 1 != 0,
        DIR_ALTERNATE_REVERSE => iter & 1 == 0,
        _ => false,
    };
    if reverse { 1.0 - raw } else { raw }
}

fn stops_build<K: KeyframeSource + ?Sized>(kf: Option<&K>) -> Vec<KfStop> {
    let Some(kf) = kf else {
        return Vec::new();
    };
    kf.stops()
        .into_iter()
        .map(|(pct, raw)| {
            let mut decls = Decls::parse(raw);
            if let Some(d) = decls.as_mut() {
                d.drop_important();
            }
            KfStop { pct, decls }
        })
        .collect()
}

struct State {
    node: NodePtr,
    chans: Vec<Chan>,
    runs: Vec<Run>,
    scripts: Vec<Run>,
    base_values: BTreeMap<i32, Val>,
    prev_style: Option<RetainedStyle>,
    has_transition: bool,
    run_generation: u32,
}

impl State {
    fn new(node: NodePtr) -> State {
        State {
            node,
            chans: Vec::new(),
            runs: Vec::new(),
            scripts: Vec::new(),
            base_values: BTreeMap::new(),
            prev_style: None,
            has_transition: false,
            run_generation: 0,
        }
    }

    fn all_runs(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter().chain(self.scripts.iter())
    }

    fn is_active(&self) -> bool {
        self.all_runs().any(|r| r.active && !r.paused)
            || self.chans.iter().any(|ch| ch.active && !ch.paused)
    }

    fn active_count(&self) -> i32 {
        let runs = self.all_runs().filter(|r| r.active).count();
        let chans = self.chans.iter().filter(|ch| ch.active).count();
        (runs + chans) as i32
    }

    fn chan_index(&self, prop: i32) -> Option<usize> {
        self.chans.iter().position(|ch| ch.prop == prop)
    }

    fn chan_ensure(&mut self, prop: i32) -> usize {
        self.chan_index(prop).unwrap_or_else(|| {
            self.chans.push(Chan::new(prop));
            self.chans.len() - 1
        })
    }

    fn prev_style_ptr(&self) -> *const southstar_layout::Style {
        self.prev_style
            .as_ref()
            .map_or(core::ptr::null(), RetainedStyle::ptr)
    }

    fn run_for(&mut self, prop: i32) -> Option<&mut Run> {
        let mut index = -1 - prop;
        let runs = if index >= SCRIPT_BASE {
            index -= SCRIPT_BASE;
            &mut self.scripts
        } else {
            &mut self.runs
        };
        let r = runs.get_mut(usize::try_from(index).ok()?)?;
        (r.name.is_some() || r.cancelled_name.is_some()).then_some(r)
    }

    fn prop_value(&self, prop: i32) -> Borrowed {
        for runs in [&self.scripts, &self.runs] {
            for r in runs.iter().rev() {
                if let Some(v) = r.values.get(&prop) {
                    return v.borrow();
                }
            }
        }
        match self.chan_index(prop).map(|i| &self.chans[i]) {
            Some(ch) if ch.active => Borrowed::of(ch.current.as_ref()),
            _ => Borrowed::NULL,
        }
    }
}

struct Shared {
    keyframes: HashMap<Vec<u8>, Keyframes>,
    active_count: i32,
    events: Vec<Event>,
    now_us: i64,
}

impl Shared {
    fn now(&mut self) -> i64 {
        if self.now_us == 0 {
            self.now_us = css::monotonic_us();
        }
        self.now_us
    }

    fn emit(&mut self, node: NodePtr, kind: &'static CStr, name: &[u8], elapsed_ms: f64) {
        let elapsed_ms = if elapsed_ms == 0.0 { 0.0 } else { elapsed_ms };
        self.events.push(Event {
            node,
            kind,
            name: name.to_vec(),
            elapsed_ms,
        });
    }

    fn emit_prop(&mut self, node: NodePtr, kind: &'static CStr, prop: i32, elapsed_ms: f64) {
        let name = css::prop_name(prop).map_or(&b""[..], CStr::to_bytes);
        self.emit(node, kind, name, elapsed_ms);
    }

    fn emit_script(&mut self, node: NodePtr, r: &Run, kind: &'static CStr) {
        self.emit(node, kind, r.index.to_string().as_bytes(), 0.0);
    }

    fn release(&mut self) {
        if self.active_count > 0 {
            self.active_count -= 1;
        }
    }

    fn chan_cancel(&mut self, node: NodePtr, ch: &mut Chan, now_us: i64) {
        let elapsed = gclamp(ch.elapsed_ms(now_us), 0.0, ch.duration_ms);
        self.emit_prop(node, c"transitioncancel", ch.prop, elapsed);
        ch.active = false;
        ch.phase = Phase::Idle;
        self.release();
    }

    fn chan_emit_phase(&mut self, node: NodePtr, ch: &mut Chan, cur: Phase) {
        let old = ch.phase;
        if old == cur {
            return;
        }
        let start_el = gmax(gmin(-ch.delay_ms, ch.duration_ms), 0.0);
        match cur {
            Phase::Active => {
                let at = if old == Phase::After {
                    ch.duration_ms
                } else {
                    start_el
                };
                self.emit_prop(node, c"transitionstart", ch.prop, at);
            }
            Phase::After => {
                if old != Phase::Active {
                    self.emit_prop(node, c"transitionstart", ch.prop, start_el);
                }
                self.emit_prop(node, c"transitionend", ch.prop, ch.duration_ms);
            }
            Phase::Before => {
                if old == Phase::After {
                    self.emit_prop(node, c"transitionstart", ch.prop, ch.duration_ms);
                }
                if old != Phase::Idle {
                    self.emit_prop(node, c"transitionend", ch.prop, 0.0);
                }
            }
            Phase::Idle => {}
        }
        ch.phase = cur;
    }

    fn advance_chan(&mut self, node: NodePtr, ch: &mut Chan, now_us: i64) -> bool {
        let elapsed = ch.elapsed_ms(now_us);
        let cur = ch.phase_at(elapsed);
        self.chan_emit_phase(node, ch, cur);
        if cur == Phase::Before {
            if Borrowed::of(ch.current.as_ref()) != Borrowed::of(ch.from.as_ref()) {
                ch.current = ch.from.clone();
            }
            return true;
        }
        ch.started = true;
        if cur == Phase::After {
            ch.current = ch.to.clone();
            if ch.active {
                ch.active = false;
                ch.finished = true;
                self.release();
            }
            return true;
        }
        let t = if ch.duration_ms > 0.0 {
            elapsed / ch.duration_ms
        } else {
            1.0
        };
        let eased = ch.timing.apply(t);
        let (from, to) = (Borrowed::of(ch.from.as_ref()), Borrowed::of(ch.to.as_ref()));
        let nearest = if eased < 0.5 { from } else { to };
        let next = if ch.discrete {
            Val::retain(nearest)
        } else {
            Val::interpolate(from, to, eased)
        };
        ch.current = next.or_else(|| Val::retain(nearest));
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn chan_start(
        &mut self,
        node: NodePtr,
        ch: &mut Chan,
        from: Borrowed,
        to: Borrowed,
        e: Entry<'_>,
        now_us: i64,
        discrete: bool,
    ) {
        let mut duration_ms = e.duration_ms();
        let mut delay_ms = e.delay_ms();
        if ch.active && ch.to.is_some() && to.equal(Borrowed::of(ch.from.as_ref())) {
            let elapsed = ch.elapsed_ms(now_us);
            let p = if ch.duration_ms > 0.0 {
                gclamp(elapsed / ch.duration_ms, 0.0, 1.0)
            } else {
                1.0
            };
            let mut factor = ch.timing.apply(p);
            if elapsed < 0.0 {
                factor = 0.0;
            }
            duration_ms *= factor;
            if delay_ms < 0.0 {
                delay_ms *= factor;
            }
        }
        if ch.active {
            self.chan_cancel(node, ch, now_us);
        }
        let new_from = Val::retain(from);
        let new_to = Val::retain(to);
        let new_current = Val::retain(from);
        ch.from = new_from;
        ch.to = new_to;
        ch.current = new_current;
        ch.active = true;
        ch.started = false;
        ch.paused = false;
        ch.finished = false;
        ch.cancelled = false;
        ch.pending = true;
        ch.phase = Phase::Idle;
        ch.discrete = discrete;
        ch.start_us = now_us as f64;
        ch.duration_ms = duration_ms;
        ch.delay_ms = delay_ms;
        ch.timing = e.timing();
        ch.generation = ch.generation.wrapping_add(1);
        self.active_count += 1;
        self.emit_prop(node, c"transitionrun", ch.prop, 0.0);
        self.advance_chan(node, ch, now_us);
    }

    fn run_cancel(&mut self, node: NodePtr, r: &mut Run) {
        let now = self.now();
        let at = r.active_time(r.elapsed_ms(now));
        if r.active {
            r.active = false;
            self.release();
        }
        if let Some(name) = r.name.take() {
            if r.is_script {
                self.emit_script(node, r, c"__nscancel");
            } else if r.phase != Phase::Idle {
                self.emit(node, c"animationcancel", &name, at);
            }
        }
        r.finished = false;
        r.started = false;
        r.pending = false;
        r.phase = Phase::Idle;
        r.clear_samples();
    }

    fn advance_run(&mut self, r: &mut Run, now_us: i64) -> bool {
        if r.name.is_none() {
            return false;
        }
        let elapsed = r.elapsed_ms(now_us);
        if elapsed < 0.0 {
            let fill_back = r.fill == FILL_BACKWARDS || r.fill == FILL_BOTH;
            if !fill_back {
                r.clear_samples();
                return false;
            }
            r.sample_at(directed_progress(0, 0.0, r.direction));
            return true;
        }
        let active = r.active_ms();
        if r.duration_ms <= 0.0 || (active.is_finite() && elapsed >= active) {
            if r.active {
                r.active = false;
                r.finished = true;
                self.release();
            }
            let fill_fwd = r.fill == FILL_FORWARDS || r.fill == FILL_BOTH;
            if !fill_fwd {
                r.clear_samples();
                return true;
            }
            let last = r.last_iteration();
            let raw_end = if r.iterations.is_finite() {
                r.iterations - f64::from(last)
            } else {
                1.0
            };
            let raw_end = gclamp(raw_end, 0.0, 1.0);
            r.sample_at(directed_progress(last, raw_end, r.direction));
            return true;
        }
        let iter_d = gmin(elapsed / r.duration_ms, 1e9);
        let iter = iter_d as i32;
        let raw = (elapsed % r.duration_ms) / r.duration_ms;
        r.sample_at(directed_progress(iter, raw, r.direction));
        true
    }

    fn run_emit_progress(&mut self, node: NodePtr, r: &mut Run, now_us: i64) {
        if r.is_script {
            if !r.active {
                self.emit_script(node, r, c"__nsfinish");
            }
            return;
        }
        let Some(name) = r.name.clone() else {
            return;
        };
        let elapsed = r.elapsed_ms(now_us);
        let old = r.phase;
        let cur = r.phase_at(elapsed);
        let active = r.active_ms();
        let start_el = gmax(
            gmin(
                -r.delay_ms,
                if active.is_finite() {
                    active
                } else {
                    -r.delay_ms
                },
            ),
            0.0,
        );
        let end_el = if active.is_finite() { active } else { elapsed };
        if old != cur {
            match cur {
                Phase::Active => {
                    let at = if old == Phase::After {
                        end_el
                    } else {
                        start_el
                    };
                    self.emit(node, c"animationstart", &name, at);
                    r.started = true;
                    r.iters_emitted = gmin(elapsed / gmax(r.duration_ms, 1e-9), 1e9) as i32;
                    if old == Phase::After {
                        r.iters_emitted = r.last_iteration();
                    }
                }
                Phase::After => {
                    if old != Phase::Active {
                        self.emit(node, c"animationstart", &name, start_el);
                    }
                    self.emit(node, c"animationend", &name, end_el);
                    r.started = false;
                }
                Phase::Before => {
                    if old == Phase::After {
                        self.emit(node, c"animationstart", &name, end_el);
                    }
                    if old != Phase::Idle {
                        self.emit(node, c"animationend", &name, 0.0);
                    }
                    r.started = false;
                }
                Phase::Idle => {}
            }
            r.phase = cur;
            return;
        }
        if cur == Phase::Active && r.duration_ms > 0.0 {
            let mut reached = gmin(elapsed / r.duration_ms, 1e9) as i32;
            let last = r.last_iteration();
            if r.iterations.is_finite() && reached > last {
                reached = last;
            }
            if r.iters_emitted < reached {
                r.iters_emitted = reached;
                self.emit(
                    node,
                    c"animationiteration",
                    &name,
                    f64::from(reached) * r.duration_ms,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_start(
        &mut self,
        node: NodePtr,
        run_generation: &mut u32,
        r: &mut Run,
        e: Entry<'_>,
        name: &CStr,
        style: StyleRef<'_>,
        now_us: i64,
    ) {
        if !r.active {
            if self.active_count >= MAX_ACTIVE {
                return;
            }
            self.active_count += 1;
        }
        r.name = Some(name.to_bytes().to_vec());
        r.cancelled_name = None;
        r.start_us = now_us as f64;
        r.configure(e);
        r.active = true;
        r.paused = e.paused();
        r.css_paused = e.paused();
        r.api_override = false;
        r.pending = !e.paused();
        r.started = false;
        r.finished = false;
        r.phase = Phase::Idle;
        r.iters_emitted = 0;
        r.elapsed_base_ms = -e.delay_ms();
        *run_generation = run_generation.wrapping_add(1);
        r.generation = *run_generation;
        r.kf = None;
        r.stops = None;
        let registered = self.keyframes.get(name.to_bytes());
        r.kf = registered.and_then(|k| k.resolve(style.vars_ptr()));
        r.stops = Some(match &r.kf {
            Some(resolved) => stops_build(Some(resolved)),
            None => stops_build(registered),
        });
        r.values.clear();
        self.advance_run(r, now_us);
        self.run_emit_progress(node, r, now_us);
    }
}

fn touch<'e>(
    p: i32,
    e: Entry<'e>,
    by_prop: &mut [Option<Entry<'e>>],
    touched: &mut [bool; PROP_COUNT],
    order: &mut Vec<i32>,
) {
    let i = p as usize;
    by_prop[i] = Some(e);
    if !touched[i] {
        touched[i] = true;
        order.push(p);
    }
}

fn entry_prop(e: Entry<'_>) -> i32 {
    let k = props::known();
    match e.target() {
        Target::Opacity => k.opacity,
        Target::Transform => k.transform,
        Target::Color => k.color,
        Target::BackgroundColor => k.background_color,
        Target::Other(name) => name.map_or(-1, css::prop_id),
        Target::All | Target::None => -1,
    }
}

fn ancestor_transitions_to(
    states: &BTreeMap<usize, State>,
    node: NodePtr,
    prop: i32,
    cur: Borrowed,
) -> bool {
    node.ancestors().any(|p| {
        let Some(ps) = states.get(&p.key()) else {
            return false;
        };
        ps.chan_index(prop).map(|i| &ps.chans[i]).is_some_and(|pc| {
            pc.active
                && (Borrowed::of(pc.to.as_ref()).equal(cur)
                    || Borrowed::of(pc.current.as_ref()).equal(cur))
        })
    })
}

fn ancestor_hidden(styles: Option<StylesTable>, node: NodePtr) -> bool {
    let Some(styles) = styles else {
        return false;
    };
    node.ancestors().any(|p| {
        p.style_in(styles)
            .is_some_and(|ps| display_of(Some(ps)).is_none())
    })
}

fn apply_propagate(styles: StylesTable, node: NodePtr, prop: i32, base: Borrowed, current: &Val) {
    let mut c = node.first_child();
    while let Some(child) = c {
        let style = child.style_in(styles);
        let inherits = style.is_some_and(|st| css::style_value(st, prop) == base);
        if let Some(st) = style.filter(|_| inherits) {
            StyleMut::of(st).set_retained(prop, current);
        }
        c = child.next_in_subtree(node, inherits);
    }
}

fn apply_animated_value(
    styles: StylesTable,
    node: NodePtr,
    base_values: &mut BTreeMap<i32, Val>,
    st: &StyleMut,
    prop: i32,
    current: &Val,
) -> bool {
    if !props::in_range(prop) {
        return false;
    }
    let base = st.value(prop);
    if base == current.borrow() {
        return false;
    }
    base_values.entry(prop).or_insert_with(|| {
        if base.is_null() {
            Val::placeholder()
        } else {
            Val::retain(base).unwrap_or_else(Val::placeholder)
        }
    });
    let old = st.replace(prop, current.clone());
    if !base.is_null() && css::prop_inherits(prop) {
        apply_propagate(styles, node, prop, base, current);
    }
    drop(old);
    true
}

pub struct Info {
    pub node: NodePtr,
    pub prop: i32,
    pub run: i32,
    pub name: Option<Vec<u8>>,
    pub prop_name: Option<&'static CStr>,
    pub fill: &'static CStr,
    pub direction: &'static CStr,
    pub easing: Vec<u8>,
    pub current_ms: f64,
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub iterations: f64,
    pub active: bool,
    pub paused: bool,
    pub pending: bool,
    pub finished: bool,
    pub generation: u32,
}

fn easing_text(timing: &Timing) -> Vec<u8> {
    timing
        .serialize()
        .map_or_else(Vec::new, |s| s.to_bytes().to_vec())
}

fn chan_info(node: NodePtr, ch: &Chan, now_us: i64) -> Info {
    let mut elapsed = ch.elapsed_ms(now_us) + ch.delay_ms;
    if ch.finished {
        elapsed = ch.delay_ms + ch.duration_ms;
    }
    Info {
        node,
        prop: ch.prop,
        run: -1,
        name: None,
        prop_name: css::prop_name(ch.prop),
        fill: c"backwards",
        direction: c"normal",
        easing: easing_text(&ch.timing),
        current_ms: elapsed,
        duration_ms: ch.duration_ms,
        delay_ms: ch.delay_ms,
        iterations: 1.0,
        active: ch.active,
        paused: ch.paused,
        pending: ch.pending,
        finished: ch.finished,
        generation: ch.generation,
    }
}

fn fill_name(fill: i32) -> &'static CStr {
    match fill {
        FILL_FORWARDS => c"forwards",
        FILL_BACKWARDS => c"backwards",
        FILL_BOTH => c"both",
        _ => c"none",
    }
}

fn direction_name(direction: i32) -> &'static CStr {
    match direction {
        DIR_REVERSE => c"reverse",
        DIR_ALTERNATE => c"alternate",
        DIR_ALTERNATE_REVERSE => c"alternate-reverse",
        _ => c"normal",
    }
}

fn run_info(node: NodePtr, r: &Run, now_us: i64) -> Info {
    let mut elapsed = r.elapsed_ms(now_us) + r.delay_ms;
    if r.finished && !r.paused {
        let active = r.active_ms();
        elapsed = gmax(
            elapsed,
            r.delay_ms + if active.is_finite() { active } else { 0.0 },
        );
    }
    Info {
        node,
        prop: -1,
        run: r.index,
        name: if r.is_script { None } else { r.name.clone() },
        prop_name: None,
        fill: fill_name(r.fill),
        direction: direction_name(r.direction),
        easing: easing_text(&r.timing),
        current_ms: elapsed,
        duration_ms: r.duration_ms,
        delay_ms: r.delay_ms,
        iterations: r.iterations,
        active: r.active,
        paused: r.paused,
        pending: r.pending,
        finished: r.finished,
        generation: r.generation,
    }
}

pub fn direction_from_name(d: Option<&CStr>) -> i32 {
    match d.map(CStr::to_bytes) {
        Some(b"reverse") => DIR_REVERSE,
        Some(b"alternate") => DIR_ALTERNATE,
        Some(b"alternate-reverse") => DIR_ALTERNATE_REVERSE,
        _ => DIR_NORMAL,
    }
}

pub fn fill_from_name(f: Option<&CStr>) -> i32 {
    match f.map(CStr::to_bytes) {
        Some(b"forwards") => FILL_FORWARDS,
        Some(b"backwards") => FILL_BACKWARDS,
        Some(b"both") => FILL_BOTH,
        _ => FILL_NONE,
    }
}

pub struct ScriptTiming<'a> {
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub iterations: f64,
    pub direction: Option<&'a CStr>,
    pub fill: Option<&'a CStr>,
    pub easing: Option<&'a CStr>,
}

pub struct KeyframeCopy {
    pub offset: f64,
    pub easing: Vec<u8>,
    pub decls: Option<Decls>,
}

pub struct Anim {
    states: BTreeMap<usize, State>,
    active: BTreeSet<usize>,
    shared: Shared,
}

impl Drop for Anim {
    fn drop(&mut self) {
        for state in self.states.values() {
            state.node.exclude_from_incremental(false);
        }
    }
}

impl Anim {
    pub fn new() -> Anim {
        Anim {
            states: BTreeMap::new(),
            active: BTreeSet::new(),
            shared: Shared {
                keyframes: HashMap::new(),
                active_count: 0,
                events: Vec::new(),
                now_us: 0,
            },
        }
    }

    fn track(&mut self, s: &State) {
        if s.is_active() {
            self.active.insert(s.node.key());
        } else {
            self.active.remove(&s.node.key());
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        core::mem::take(&mut self.shared.events)
    }

    pub fn load_keyframes(&mut self, sheet: *const core::ffi::c_void) {
        for (name, kf) in css::sheet_keyframes(sheet) {
            self.shared.keyframes.insert(name, kf);
        }
    }

    pub fn prune(&mut self, live: StylesTable) {
        let dead: Vec<usize> = self
            .states
            .iter()
            .filter(|(_, s)| !live.contains(s.node))
            .map(|(&k, _)| k)
            .collect();
        for key in dead {
            if let Some(s) = self.states.remove(&key) {
                self.shared.active_count -= s.active_count();
                if self.shared.active_count < 0 {
                    self.shared.active_count = 0;
                }
                self.active.remove(&key);
                s.node.exclude_from_incremental(false);
            }
        }
    }

    pub fn rebase(&mut self, base_us: i64) {
        for s in self.states.values_mut() {
            for r in s.runs.iter_mut().chain(s.scripts.iter_mut()) {
                r.start_us = base_us as f64;
            }
            for ch in &mut s.chans {
                ch.start_us = base_us as f64;
            }
        }
    }

    fn observe_transition_prop(
        &mut self,
        s: &mut State,
        style: StyleRef<'_>,
        prop: i32,
        e: Option<Entry<'_>>,
        now_us: i64,
    ) {
        let cur = css::style_value(style, prop);
        let index = s.chan_ensure(prop);
        let node = s.node;
        let prev_style = s.prev_style.as_ref().map(RetainedStyle::get);
        let ch = &mut s.chans[index];
        if !cur.is_null() && ch.current.is_some() && cur == Borrowed::of(ch.current.as_ref()) {
            return;
        }
        if !ch.has_last
            && let Some(prev) = prev_style.filter(|p| p.as_ptr() != style.as_ptr())
        {
            ch.last = Val::retain(css::style_value(prev, prop));
            ch.last_currentcolor = css::from_currentcolor(prev, prop);
            ch.has_last = true;
        }
        if !ch.has_last
            && let Some(before) = node
                .style_before_change()
                .filter(|b| b.as_ptr() != style.as_ptr())
        {
            ch.last = Val::retain(css::style_value(before, prop));
            ch.last_currentcolor = css::from_currentcolor(before, prop);
            ch.has_last = true;
        }
        if !ch.has_last {
            ch.last = Val::retain(cur);
            ch.last_currentcolor = css::from_currentcolor(style, prop);
            ch.has_last = true;
            return;
        }
        let cur_currentcolor = css::from_currentcolor(style, prop);
        let last = Borrowed::of(ch.last.as_ref());
        if cur.equal(last) || (cur_currentcolor && ch.last_currentcolor) {
            ch.last_currentcolor = cur_currentcolor;
            if !cur.equal(last) {
                ch.last = Val::retain(cur);
            }
            return;
        }
        let cur_init = if cur.is_null() {
            css::initial_value(prop)
        } else {
            None
        };
        let last_init = if ch.last.is_none() {
            css::initial_value(prop)
        } else {
            None
        };
        let cur_eff = if cur.is_null() {
            Borrowed::of(cur_init.as_ref())
        } else {
            cur
        };
        let last_eff = if ch.last.is_some() {
            last
        } else {
            Borrowed::of(last_init.as_ref())
        };
        let can_run = e.is_some_and(|e| {
            !cur_eff.is_null()
                && !last_eff.is_null()
                && e.duration_ms() + e.delay_ms() > 0.0
                && e.duration_ms() >= 0.0
                && !css::reduced_motion()
                && (ch.active || self.shared.active_count < MAX_ACTIVE)
        });
        match e.filter(|_| can_run) {
            Some(e) => {
                let from = if ch.active {
                    Borrowed::of(ch.current.as_ref())
                } else {
                    last_eff
                };
                let interpolable = Val::interpolate(from, cur_eff, 0.0).is_some();
                if interpolable && ancestor_transitions_to(&self.states, node, prop, cur_eff) {
                    if ch.active {
                        self.shared.chan_cancel(node, ch, now_us);
                    }
                } else if interpolable {
                    self.shared
                        .chan_start(node, ch, from, cur_eff, e, now_us, false);
                } else if props::discretely_animatable(prop) || e.allow_discrete() {
                    self.shared
                        .chan_start(node, ch, from, cur_eff, e, now_us, true);
                } else if ch.active {
                    self.shared.chan_cancel(node, ch, now_us);
                }
            }
            None => {
                if ch.active {
                    self.shared.chan_cancel(node, ch, now_us);
                }
            }
        }
        drop(cur_init);
        drop(last_init);
        ch.last = Val::retain(cur);
        ch.last_currentcolor = cur_currentcolor;
    }

    fn observe_transition(
        &mut self,
        s: &mut State,
        style: StyleRef<'_>,
        tv: &AnimList,
        now_us: i64,
    ) {
        s.has_transition = tv.len() > 0;
        let entries = tv.entries();
        let mut by_prop: Vec<Option<Entry<'_>>> = vec![None; PROP_COUNT];
        let mut touched = [false; PROP_COUNT];
        let mut order = Vec::new();
        for &e in &entries {
            if matches!(e.target(), Target::All) {
                for p in 0..PROP_COUNT as i32 {
                    if props::transitionable(p) {
                        touch(p, e, &mut by_prop, &mut touched, &mut order);
                    }
                }
                continue;
            }
            let p = entry_prop(e);
            if props::transitionable(p) {
                touch(p, e, &mut by_prop, &mut touched, &mut order);
            }
        }
        for &p in &order {
            let e = by_prop[p as usize];
            self.observe_transition_prop(s, style, p, e, now_us);
        }
        for i in 0..s.chans.len() {
            let prop = s.chans[i].prop;
            if !props::in_range(prop) || touched[prop as usize] {
                continue;
            }
            if s.chans[i].active {
                let node = s.node;
                self.shared.chan_cancel(node, &mut s.chans[i], now_us);
                s.chans[i].cancelled = true;
            }
            self.observe_transition_prop(s, style, prop, None, now_us);
        }
    }

    fn observe_animation(
        &mut self,
        s: &mut State,
        style: StyleRef<'_>,
        av: &AnimList,
        now_us: i64,
    ) {
        let entries = av.entries();
        let n = if css::reduced_motion() {
            0
        } else {
            entries.len()
        };
        if s.runs.is_empty() && n == 0 {
            return;
        }
        let node = s.node;
        for (i, &e) in entries.iter().enumerate().take(n) {
            while s.runs.len() <= i {
                let index = s.runs.len() as i32;
                s.runs.push(Run::new(index, false));
            }
            let r = &mut s.runs[i];
            let Some(name) = e.name().filter(|name| {
                e.duration_ms().partial_cmp(&0.0) != Some(Ordering::Less)
                    && self.shared.keyframes.contains_key(name.to_bytes())
            }) else {
                if r.name.is_some() {
                    self.shared.run_cancel(node, r);
                }
                r.cancelled_name = None;
                continue;
            };
            if let Some(cancelled) = &r.cancelled_name {
                if cancelled.as_slice() == name.to_bytes() {
                    continue;
                }
                r.cancelled_name = None;
            }
            if r.name.as_deref() == Some(name.to_bytes()) {
                r.configure(e);
                if e.paused() != r.css_paused && !r.api_override {
                    r.set_paused(e.paused(), now_us);
                }
                r.css_paused = e.paused();
                continue;
            }
            if r.name.is_some() {
                self.shared.run_cancel(node, r);
            }
            let (runs, generation) = (&mut s.runs, &mut s.run_generation);
            self.shared
                .run_start(node, generation, &mut runs[i], e, name, style, now_us);
        }
        for r in s.runs.iter_mut().skip(n) {
            if r.name.is_some() {
                self.shared.run_cancel(node, r);
            }
            r.cancelled_name = None;
        }
        while s
            .runs
            .last()
            .is_some_and(|last| last.name.is_none() && last.cancelled_name.is_none())
        {
            s.runs.pop();
        }
    }

    pub fn observe(
        &mut self,
        node: NodePtr,
        style: StyleRef<'_>,
        now_us: i64,
        styles: Option<StylesTable>,
    ) {
        if self.shared.now_us == 0 || self.active.is_empty() {
            self.shared.now_us = now_us;
        }
        let now_us = self.shared.now_us;
        if let Some(s) = self.states.get(&node.key())
            && s.prev_style_ptr() == style.as_ptr()
            && !s.is_active()
        {
            return;
        }
        let mut tv = AnimList::effective(style, false);
        let mut av = AnimList::effective(style, true);
        let mut s = match self.states.remove(&node.key()) {
            Some(s) => s,
            None => {
                if tv.len() == 0 && av.len() == 0 {
                    return;
                }
                State::new(node)
            }
        };
        if display_of(Some(style)).is_none() || ancestor_hidden(styles, node) {
            tv.clear();
            av.clear();
            for ch in &mut s.chans {
                if ch.active {
                    self.shared.chan_cancel(node, ch, now_us);
                }
                ch.finished = false;
                ch.cancelled = true;
            }
        }
        self.observe_transition(&mut s, style, &tv, now_us);
        self.observe_animation(&mut s, style, &av, now_us);
        self.track(&s);
        drop(tv);
        drop(av);
        if s.prev_style_ptr() != style.as_ptr() {
            s.prev_style = Some(RetainedStyle::retain(style));
        }
        self.states.insert(node.key(), s);
    }

    pub fn observe_all(&mut self, styles: StylesTable, now_us: i64) {
        if self.shared.now_us == 0 || self.active.is_empty() {
            self.shared.now_us = now_us;
        }
        let mut items: Vec<(NodePtr, StyleRef<'static>, usize)> = styles
            .entries()
            .into_iter()
            .filter(|(node, style)| match self.states.get(&node.key()) {
                Some(s) => s.prev_style_ptr() != style.as_ptr() || s.is_active(),
                None => css::may_animate(*style),
            })
            .map(|(node, style)| (node, style, node.ancestors().count()))
            .collect();
        items.sort_by_key(|item| item.2);
        for (node, style, _) in items {
            self.observe(node, style, now_us, Some(styles));
        }
        self.prune(styles);
        self.apply(styles);
    }

    pub fn base_value(&self, node: NodePtr, prop: i32) -> Borrowed {
        let Some(v) = self
            .states
            .get(&node.key())
            .and_then(|s| s.base_values.get(&prop))
        else {
            return Borrowed::NULL;
        };
        if v.borrow().is_placeholder() {
            Borrowed::NULL
        } else {
            v.borrow()
        }
    }

    pub fn apply(&mut self, styles: StylesTable) {
        for s in self.states.values_mut() {
            let Some(style) = s.node.style_in(styles) else {
                continue;
            };
            let st = StyleMut::of(style);
            let node = s.node;
            let mut mutated = false;
            s.base_values.clear();
            let State {
                chans,
                runs,
                scripts,
                base_values,
                ..
            } = s;
            for ch in chans.iter() {
                if let (true, Some(current)) = (ch.active, ch.current.as_ref()) {
                    mutated |=
                        apply_animated_value(styles, node, base_values, &st, ch.prop, current);
                }
            }
            for r in runs.iter_mut().chain(scripts.iter_mut()) {
                let Run {
                    partials, values, ..
                } = r;
                for (&prop, pa) in partials.iter() {
                    let mut base = base_values.get(&prop).map_or(Borrowed::NULL, Val::borrow);
                    if base.is_placeholder() {
                        base = Borrowed::NULL;
                    }
                    if base.is_null() {
                        base = st.value(prop);
                    }
                    let init = if base.is_null() {
                        css::initial_value(prop)
                    } else {
                        None
                    };
                    let fallback = if base.is_null() {
                        Borrowed::of(init.as_ref())
                    } else {
                        base
                    };
                    let from = pa.from.as_ref().map_or(fallback, Val::borrow);
                    let to = pa.to.as_ref().map_or(fallback, Val::borrow);
                    let both = !from.is_null() && !to.is_null();
                    let mut out = if both {
                        Val::interpolate(from, to, pa.t)
                    } else {
                        None
                    };
                    if out.is_none() && both {
                        out = Val::retain(if pa.t < 0.5 { from } else { to });
                    }
                    drop(init);
                    if let Some(out) = out {
                        values.insert(prop, out);
                    }
                }
                for (&prop, v) in values.iter() {
                    mutated |= apply_animated_value(styles, node, base_values, &st, prop, v);
                }
            }
            node.exclude_from_incremental(mutated);
        }
    }

    pub fn tick(&mut self, now_us: i64) -> bool {
        self.shared.now_us = now_us;
        if self.active.is_empty() {
            return false;
        }
        let mut any = false;
        let keys: Vec<usize> = self.active.iter().copied().collect();
        for key in keys {
            let Some(s) = self.states.get_mut(&key) else {
                self.active.remove(&key);
                continue;
            };
            let node = s.node;
            for ch in s.chans.iter_mut() {
                if !ch.active || ch.paused {
                    continue;
                }
                if ch.pending {
                    ch.pending = false;
                    ch.start_us = now_us as f64;
                }
                if self.shared.advance_chan(node, ch, now_us) {
                    any = true;
                }
            }
            for r in s.runs.iter_mut().chain(s.scripts.iter_mut()) {
                if !r.active {
                    continue;
                }
                if r.pending {
                    r.pending = false;
                    r.start_us = now_us as f64;
                }
                if self.shared.advance_run(r, now_us) {
                    any = true;
                }
                self.shared.run_emit_progress(node, r, now_us);
            }
            if !s.is_active() {
                self.active.remove(&key);
            }
        }
        any
    }

    pub fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    pub fn needs_layout(&self) -> bool {
        self.active
            .iter()
            .filter_map(|k| self.states.get(k))
            .any(|s| {
                s.chans
                    .iter()
                    .any(|ch| ch.active && props::needs_relayout(ch.prop))
                    || s.all_runs().any(|r| {
                        r.partials.keys().any(|&p| props::needs_relayout(p))
                            || r.values.keys().any(|&p| props::needs_relayout(p))
                    })
            })
    }

    fn prop_value(&self, node: NodePtr, prop: i32) -> Borrowed {
        self.states
            .get(&node.key())
            .map_or(Borrowed::NULL, |s| s.prop_value(prop))
    }

    pub fn has_state(&self, node: NodePtr) -> bool {
        self.states.contains_key(&node.key())
    }

    pub fn opacity(&self, node: NodePtr) -> Option<f64> {
        self.prop_value(node, props::known().opacity)
            .opacity()
            .map(|o| gclamp(o, 0.0, 1.0))
    }

    pub fn transform(&self, node: NodePtr) -> Option<*const core::ffi::c_void> {
        self.prop_value(node, props::known().transform).transform()
    }

    pub fn color(&self, node: NodePtr, which: i32) -> Option<[u8; 4]> {
        let k = props::known();
        let prop = match which {
            TARGET_COLOR => k.color,
            TARGET_BG_COLOR => k.background_color,
            _ => return None,
        };
        self.prop_value(node, prop).color()
    }

    pub fn visit(&mut self, filter: Option<NodePtr>) -> Vec<Info> {
        let now = self.shared.now();
        let mut infos = Vec::new();
        for s in self.states.values() {
            if filter.is_some_and(|f| f != s.node) {
                continue;
            }
            for r in s.all_runs().filter(|r| r.in_effect()) {
                infos.push(run_info(s.node, r, now));
            }
            for ch in s.chans.iter().filter(|ch| ch.active) {
                infos.push(chan_info(s.node, ch, now));
            }
        }
        infos
    }

    pub fn info_for(&mut self, node: NodePtr, prop: i32) -> Option<Info> {
        if !self.has_state(node) {
            return None;
        }
        let now = self.shared.now();
        let s = self.states.get_mut(&node.key())?;
        if prop < 0 {
            let r = s.run_for(prop)?;
            r.name.as_ref()?;
            return Some(run_info(node, r, now));
        }
        let ch = s.chan_index(prop).map(|i| &s.chans[i])?;
        if !ch.active && !ch.finished {
            return None;
        }
        Some(chan_info(node, ch, now))
    }

    pub fn script_start(
        &mut self,
        node: NodePtr,
        stops: &[(Option<&CStr>, f64)],
        t: &ScriptTiming<'_>,
    ) -> Option<(i32, u32)> {
        if self.shared.active_count >= MAX_ACTIVE {
            return None;
        }
        let mut s = self
            .states
            .remove(&node.key())
            .unwrap_or_else(|| State::new(node));
        let slot = s
            .scripts
            .iter()
            .position(|c| c.name.is_none() && c.cancelled_name.is_none() && !c.active)
            .unwrap_or_else(|| {
                let index = SCRIPT_BASE + s.scripts.len() as i32;
                s.scripts.push(Run::new(index, true));
                s.scripts.len() - 1
            });
        let now = self.shared.now();
        s.run_generation = s.run_generation.wrapping_add(1);
        let generation = s.run_generation;
        let r = &mut s.scripts[slot];
        r.name = Some(Vec::new());
        r.cancelled_name = None;
        r.kf = None;
        r.stops = Some(
            stops
                .iter()
                .map(|&(css_text, pct)| KfStop {
                    pct: pct * 100.0,
                    decls: Decls::parse(css_text),
                })
                .collect(),
        );
        r.start_us = now as f64;
        r.duration_ms = gmax(t.duration_ms, 0.0);
        r.delay_ms = t.delay_ms;
        r.iterations = if t.iterations < 0.0 {
            0.0
        } else {
            t.iterations
        };
        r.direction = direction_from_name(t.direction);
        r.fill = fill_from_name(t.fill);
        r.timing = Timing::linear();
        if let Some(easing) = t.easing {
            r.timing.parse_into(easing);
        }
        r.active = true;
        r.paused = false;
        r.pending = true;
        r.started = false;
        r.finished = false;
        r.iters_emitted = 0;
        r.elapsed_base_ms = 0.0;
        r.generation = generation;
        r.values.clear();
        self.shared.active_count += 1;
        self.shared.advance_run(r, now);
        let index = r.index;
        self.track(&s);
        self.states.insert(node.key(), s);
        Some((-1 - index, generation))
    }

    pub fn keyframes(&mut self, node: NodePtr, prop: i32) -> Vec<KeyframeCopy> {
        let Some(s) = self.states.get_mut(&node.key()) else {
            return Vec::new();
        };
        let Some(r) = s.run_for(prop) else {
            return Vec::new();
        };
        let (Some(_), Some(stops)) = (&r.name, &r.stops) else {
            return Vec::new();
        };
        stops
            .iter()
            .map(|st| KeyframeCopy {
                offset: st.pct / 100.0,
                easing: easing_text(&st.timing().unwrap_or(r.timing)),
                decls: st.decls.as_ref().map(Decls::duplicate),
            })
            .collect()
    }

    pub fn seek(&mut self, node: NodePtr, prop: i32, ms: f64) -> bool {
        let Some(mut s) = self.states.remove(&node.key()) else {
            return false;
        };
        let handled = self.seek_state(&mut s, prop, ms);
        if handled {
            self.track(&s);
        }
        self.states.insert(node.key(), s);
        handled
    }

    fn seek_state(&mut self, s: &mut State, prop: i32, ms: f64) -> bool {
        let now = self.shared.now();
        let node = s.node;
        if prop < 0 {
            let Some(r) = s.run_for(prop) else {
                return false;
            };
            if r.name.is_none() {
                r.name = r.cancelled_name.take();
                r.active = true;
                r.finished = false;
                r.paused = true;
                r.pending = false;
                r.elapsed_base_ms = ms - r.delay_ms;
                self.shared.active_count += 1;
                self.shared.advance_run(r, now);
                self.shared.run_emit_progress(node, r, now);
                return true;
            }
            let elapsed = ms - r.delay_ms;
            r.pending = false;
            if r.paused {
                r.elapsed_base_ms = elapsed;
            } else {
                r.start_us = now as f64 - ms * 1000.0;
            }
            if !r.active && r.finished {
                r.active = true;
                r.finished = false;
                self.shared.active_count += 1;
            }
            self.shared.advance_run(r, now);
            self.shared.run_emit_progress(node, r, now);
            return true;
        }
        let Some(ch) = s.chan_index(prop).map(|i| &mut s.chans[i]) else {
            return false;
        };
        if !ch.active && !ch.finished {
            return false;
        }
        let elapsed = ms - ch.delay_ms;
        ch.pending = false;
        if ch.paused {
            ch.paused_elapsed_ms = elapsed;
        } else {
            ch.start_us = now as f64 - ms * 1000.0;
        }
        if !ch.active {
            ch.active = true;
            ch.finished = false;
            self.shared.active_count += 1;
        }
        self.shared.advance_chan(node, ch, now);
        true
    }

    pub fn control(&mut self, node: NodePtr, prop: i32, op: &[u8]) -> bool {
        let Some(mut s) = self.states.remove(&node.key()) else {
            return false;
        };
        let handled = self.control_state(&mut s, prop, op);
        self.states.insert(node.key(), s);
        handled
    }

    fn control_state(&mut self, s: &mut State, prop: i32, op: &[u8]) -> bool {
        let now = self.shared.now();
        let node = s.node;
        if prop < 0 {
            let Some(r) = s.run_for(prop) else {
                return false;
            };
            match op {
                b"pause" => {
                    if r.name.is_none() {
                        return false;
                    }
                    if r.pending {
                        r.pending = false;
                        r.start_us = now as f64;
                    }
                    r.set_paused(true, now);
                    r.api_override = true;
                }
                b"play" => {
                    r.api_override = true;
                    if r.name.is_none() {
                        r.name = r.cancelled_name.take();
                        r.active = true;
                        r.finished = false;
                        r.paused = false;
                        r.pending = true;
                        r.started = false;
                        r.start_us = now as f64;
                        self.shared.active_count += 1;
                        self.shared.advance_run(r, now);
                    } else {
                        r.set_paused(false, now);
                        if !r.active && r.finished {
                            r.active = true;
                            r.finished = false;
                            self.shared.active_count += 1;
                            r.start_us = now as f64;
                        }
                    }
                }
                b"finish" => {
                    if r.name.is_none() {
                        return false;
                    }
                    let active = r.active_ms();
                    let total = r.delay_ms + if active.is_finite() { active } else { 0.0 };
                    r.paused = false;
                    r.pending = false;
                    r.start_us = now as f64 - total * 1000.0;
                    self.shared.advance_run(r, now);
                    self.shared.run_emit_progress(node, r, now);
                }
                b"cancel" => {
                    let Some(keep) = r.name.clone() else {
                        return false;
                    };
                    self.shared.run_cancel(node, r);
                    r.cancelled_name = Some(keep);
                }
                _ => {}
            }
            self.track(s);
            return true;
        }
        let Some(ch) = s.chan_index(prop).map(|i| &mut s.chans[i]) else {
            return false;
        };
        if !ch.active && !ch.finished && !ch.cancelled {
            return false;
        }
        if ch.cancelled && op == b"play" {
            ch.cancelled = false;
            ch.active = true;
            ch.finished = false;
            ch.paused = false;
            ch.started = false;
            ch.phase = Phase::Idle;
            ch.start_us = now as f64;
            ch.current = ch.from.clone();
            self.shared.active_count += 1;
            self.shared.emit_prop(node, c"transitionrun", ch.prop, 0.0);
            self.shared.advance_chan(node, ch, now);
            self.track(s);
            return true;
        }
        if ch.cancelled {
            return false;
        }
        match op {
            b"pause" if !ch.paused => {
                ch.paused_elapsed_ms = ch.elapsed_ms(now);
                ch.pending = false;
                ch.paused = true;
            }
            b"play" if ch.paused => {
                ch.start_us = now as f64 - (ch.paused_elapsed_ms + ch.delay_ms) * 1000.0;
                ch.paused = false;
            }
            b"finish" => {
                ch.paused = false;
                ch.pending = false;
                ch.start_us = now as f64 - (ch.delay_ms + ch.duration_ms) * 1000.0;
                if ch.active {
                    self.shared.advance_chan(node, ch, now);
                }
            }
            b"cancel" => {
                if ch.active {
                    self.shared.chan_cancel(node, ch, now);
                }
                ch.finished = false;
                ch.cancelled = true;
            }
            _ => {}
        }
        self.track(s);
        true
    }
}
