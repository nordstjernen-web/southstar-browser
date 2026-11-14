//! Southstar — the SVG SMIL animation timeline: animate, set, animateTransform and animateMotion sampled into per-node attribute overrides.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod parse;
mod values;

use core::cell::{Cell, RefCell};
use core::cmp::Ordering;
use core::ffi::CStr;
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, WeakNode, ancestors_and_self, index};

pub(crate) use values::PathPoint;

const KEPT_INSTANCES: usize = 16;
const SYNC_PASSES: usize = 32;
const TIME_EPSILON: f64 = 1e-6;
const MAX_EVENTS: usize = 4096;
const TAGS: [&CStr; 4] = [c"animate", c"set", c"animatetransform", c"animatemotion"];

pub const MOTION_ATTR: &CStr = c"\x01motion";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Animate,
    Set,
    Transform,
    Motion,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Calc {
    Linear,
    Discrete,
    Paced,
    Spline,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Restart {
    Always,
    WhenIdle,
    Never,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpecKind {
    Offset,
    SyncBegin,
    SyncEnd,
    Event,
}

struct Spec {
    kind: SpecKind,
    offset: f64,
    id: Option<CString>,
    event: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Begin,
    End,
    Repeat,
}

impl EventKind {
    pub fn event_type(self) -> &'static CStr {
        match self {
            EventKind::Begin => c"beginEvent",
            EventKind::End => c"endEvent",
            EventKind::Repeat => c"repeatEvent",
        }
    }

    pub fn handler(self) -> &'static CStr {
        match self {
            EventKind::Begin => c"onbegin",
            EventKind::End => c"onend",
            EventKind::Repeat => c"onrepeat",
        }
    }
}

struct QueuedEvent {
    node: WeakNode,
    kind: EventKind,
    iteration: i32,
}

#[derive(Default)]
struct Clock {
    paused: bool,
    paused_at_us: i64,
    offset_us: i64,
}

struct Element {
    node: WeakNode,
    target: Option<WeakNode>,
    svg: Option<WeakNode>,
    kind: Kind,
    calc: Calc,
    restart: Restart,
    attr: Option<CString>,
    transform_type: String,
    begin_src: Vec<u8>,
    values: Vec<String>,
    key_times: Vec<f64>,
    key_splines: Vec<f64>,
    key_points: Vec<f64>,
    begins: Vec<Spec>,
    ends: Vec<f64>,
    end_specs: Vec<Spec>,
    end_instances: Vec<f64>,
    instances: Vec<f64>,
    path: Vec<PathPoint>,
    dur: f64,
    repeat_count: f64,
    repeat_dur: f64,
    forced_begin: f64,
    forced_end: f64,
    horizon: f64,
    rotate: f64,
    reported_begin: f64,
    reported_iteration: f64,
    reported: bool,
    reported_end: bool,
    has_forced_end: bool,
    is_color: bool,
    to_anim: bool,
    additive: bool,
    accumulate: bool,
    freeze: bool,
    rotate_auto: bool,
    rotate_reverse: bool,
}

#[derive(PartialEq, Eq)]
struct Override {
    name: CString,
    value: CString,
}

type Overrides = HashMap<WeakNode, Vec<Override>>;
type Inputs = Vec<(Option<WeakNode>, u32)>;

struct Running {
    index: usize,
    begin: f64,
    active_time: f64,
    ended: bool,
}

struct Timeline {
    doc: WeakNode,
    els: Vec<Element>,
    by_node: HashMap<WeakNode, usize>,
    clocks: HashMap<WeakNode, Clock>,
    overrides: Overrides,
    events: Vec<QueuedEvent>,
    inputs: Inputs,
    origin_us: i64,
    now_us: i64,
    scanned: bool,
    dirty: bool,
    active: bool,
    unreported: bool,
    has_event_specs: bool,
}

thread_local! {
    static TIMELINES: RefCell<Vec<Rc<RefCell<Timeline>>>> = const { RefCell::new(Vec::new()) };
    static OVERRIDE_NODES: Cell<usize> = const { Cell::new(0) };
    static EVENT_DOCS: Cell<usize> = const { Cell::new(0) };
}

fn is_svg(n: Node<'_>) -> bool {
    n.element_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(b"svg"))
}

fn outer_svg(n: Node<'_>) -> Option<Node<'_>> {
    ancestors_and_self(n).filter(|&p| is_svg(p)).last()
}

fn time_insert(times: &mut Vec<f64>, at: f64, horizon: f64) -> bool {
    if !at.is_finite() || at <= horizon {
        return false;
    }
    let mut i = 0;
    while i < times.len() {
        if (times[i] - at).abs() < TIME_EPSILON {
            return false;
        }
        if times[i] > at {
            break;
        }
        i += 1;
    }
    times.insert(i, at);
    true
}

fn prune_times(times: &mut Vec<f64>, t: f64) {
    let past = times.iter().take_while(|&&v| v <= t).count();
    if past > KEPT_INSTANCES {
        times.drain(..past - KEPT_INSTANCES);
    }
}

fn has_event_spec(specs: &[Spec]) -> bool {
    specs.iter().any(|s| s.kind == SpecKind::Event)
}

fn event_bubbles(kind: &[u8]) -> bool {
    const STAYS: [&[u8]; 8] = [
        b"mouseenter",
        b"mouseleave",
        b"focus",
        b"blur",
        b"load",
        b"beginEvent",
        b"endEvent",
        b"repeatEvent",
    ];
    !STAYS.contains(&kind)
}

fn event_reaches(base: Option<Node<'_>>, target: Node<'_>, bubbles: bool) -> bool {
    let Some(base) = base else {
        return false;
    };
    base == target || (bubbles && ancestors_and_self(target).any(|p| p == base))
}

impl Element {
    fn instance_add(&mut self, at: f64) -> bool {
        time_insert(&mut self.instances, at, self.horizon)
    }

    fn seed_instances(&mut self) {
        for i in 0..self.begins.len() {
            if self.begins[i].kind == SpecKind::Offset {
                self.instance_add(self.begins[i].offset);
            }
        }
    }

    fn inherit_runtime(&mut self, old: &mut Element) {
        self.instances = core::mem::take(&mut old.instances);
        self.end_instances = core::mem::take(&mut old.end_instances);
        self.reported = old.reported;
        self.reported_end = old.reported_end;
        self.reported_begin = old.reported_begin;
        self.reported_iteration = old.reported_iteration;
        self.horizon = old.horizon;
        self.has_forced_end = old.has_forced_end;
        self.forced_begin = old.forced_begin;
        self.forced_end = old.forced_end;
    }

    fn active_duration(&self) -> f64 {
        if self.dur < 0.0 {
            return if self.repeat_dur >= 0.0 {
                self.repeat_dur
            } else {
                f64::INFINITY
            };
        }
        let mut active = self.dur;
        if self.repeat_count >= 0.0 {
            active = self.dur * self.repeat_count;
        }
        if self.repeat_dur >= 0.0 {
            active = if self.repeat_count >= 0.0 {
                active.min(self.repeat_dur)
            } else {
                self.repeat_dur
            };
        }
        active
    }

    fn interval_end(&self, begin: f64) -> f64 {
        let mut end = begin + self.active_duration();
        for &v in self.ends.iter().chain(&self.end_instances) {
            if v >= begin && v < end {
                end = v;
            }
        }
        if self.has_forced_end
            && (self.forced_begin - begin).abs() < TIME_EPSILON
            && self.forced_end < end
        {
            end = self.forced_end;
        }
        end
    }

    fn current_begin(&self, t: f64) -> Option<f64> {
        let mut current = None;
        for &b in &self.instances {
            if b > t {
                break;
            }
            let Some(cur) = current else {
                current = Some(b);
                continue;
            };
            match self.restart {
                Restart::Never => break,
                Restart::WhenIdle if b < self.interval_end(cur) => {}
                _ => current = Some(b),
            }
        }
        current
    }

    fn has_future_begin(&self, t: f64) -> bool {
        self.instances.last().is_some_and(|&last| last > t)
    }

    fn prune_instances(&mut self, t: f64) {
        let past = self.instances.iter().take_while(|&&v| v <= t).count();
        if past <= KEPT_INSTANCES {
            return;
        }
        let drop = past - KEPT_INSTANCES;
        self.horizon = self.instances[drop - 1];
        self.instances.drain(..drop);
    }
}

impl Timeline {
    fn document(&self) -> Option<Node<'static>> {
        self.doc.get()
    }

    fn set_event_specs(&mut self, has: bool) {
        if self.has_event_specs == has {
            return;
        }
        self.has_event_specs = has;
        EVENT_DOCS.with(|n| n.set(if has { n.get() + 1 } else { n.get() - 1 }));
    }

    fn swap_overrides(&mut self, next: Overrides) {
        OVERRIDE_NODES.with(|n| n.set(n.get() - self.overrides.len() + next.len()));
        self.overrides = next;
    }

    fn animation_nodes(doc: Node<'static>) -> Vec<Node<'static>> {
        if !index::has_tag_index(doc) {
            index::tag_build(doc);
        }
        let mut nodes = Vec::new();
        for tag in TAGS {
            index::tag_lookup(doc, tag, |list| {
                nodes.extend(list.iter().filter(|&n| outer_svg(n).is_some()));
            });
        }
        if nodes.len() > 1 {
            nodes.sort_by(|&a, &b| index::document_order_cmp(Some(a), Some(b)).cmp(&0));
        }
        nodes
    }

    fn inputs_of(doc: Node<'static>, nodes: &[Node<'static>]) -> Inputs {
        let mut inputs = Inputs::new();
        for &n in nodes {
            inputs.push((Some(n.downgrade()), n.attr_gen()));
            inputs.push((parse::target_of(doc, n).map(Node::downgrade), 0));
            if let Some(mpath) = parse::mpath_of(n) {
                inputs.push((Some(mpath.downgrade()), mpath.attr_gen()));
                let path = parse::href_target(doc, mpath);
                inputs.push((path.map(Node::downgrade), path.map_or(0, Node::attr_gen)));
            }
        }
        inputs
    }

    fn scan(&mut self) {
        let Some(doc) = self.document() else {
            return;
        };
        let nodes = Timeline::animation_nodes(doc);
        let inputs = Timeline::inputs_of(doc, &nodes);
        if self.scanned && inputs == self.inputs {
            return;
        }
        self.inputs = inputs;
        self.scanned = true;

        let mut old_els: Vec<Option<Element>> = core::mem::take(&mut self.els)
            .into_iter()
            .map(Some)
            .collect();
        let old_by_node = core::mem::take(&mut self.by_node);
        let mut event_specs = false;
        for n in nodes {
            let Some(kind) = n.element_name().and_then(parse::kind_of) else {
                continue;
            };
            let mut el = parse::element(doc, n, kind);
            let old = old_by_node
                .get(&el.node)
                .and_then(|&i| old_els[i].take())
                .filter(|old| old.begin_src == el.begin_src);
            match old {
                Some(mut old) => el.inherit_runtime(&mut old),
                None => el.seed_instances(),
            }
            event_specs |= has_event_spec(&el.begins) || has_event_spec(&el.end_specs);
            self.by_node.insert(el.node, self.els.len());
            self.els.push(el);
        }
        self.set_event_specs(event_specs);
        self.clocks.retain(|svg, _| svg.get().is_some());
    }

    fn time_at(&self, svg: Option<WeakNode>, now_us: i64) -> f64 {
        let clock = svg.and_then(|svg| self.clocks.get(&svg));
        let (at, offset) = match clock {
            Some(c) => (if c.paused { c.paused_at_us } else { now_us }, c.offset_us),
            None => (now_us, 0),
        };
        (at - self.origin_us - offset) as f64 / 1e6
    }

    fn clock_paused(&self, svg: Option<WeakNode>) -> bool {
        svg.and_then(|svg| self.clocks.get(&svg))
            .is_some_and(|c| c.paused)
    }

    fn el_by_id(&self, id: &CStr) -> Option<usize> {
        let node = index::find_by_id(self.document()?, id)?;
        self.by_node.get(&node.downgrade()).copied()
    }

    fn sync_times(&self, index: usize, ends: bool, t: f64) -> Vec<f64> {
        let el = &self.els[index];
        let specs = if ends { &el.end_specs } else { &el.begins };
        let mut found = Vec::new();
        for spec in specs {
            if !matches!(spec.kind, SpecKind::SyncBegin | SpecKind::SyncEnd) {
                continue;
            }
            let Some(base) = spec.id.as_deref().and_then(|id| self.el_by_id(id)) else {
                continue;
            };
            if base == index {
                continue;
            }
            let base = &self.els[base];
            for &instance in base.instances.iter().take_while(|&&at| at <= t) {
                let mut at = instance;
                if spec.kind == SpecKind::SyncEnd {
                    at = base.interval_end(at);
                }
                at += spec.offset;
                if at <= t {
                    found.push(at);
                }
            }
        }
        found
    }

    fn resolve_sync(&mut self) {
        for _ in 0..SYNC_PASSES {
            let mut grew = false;
            for i in 0..self.els.len() {
                let t = self.time_at(self.els[i].svg, self.now_us);
                let begins = self.sync_times(i, false, t);
                let ends = self.sync_times(i, true, t);
                let el = &mut self.els[i];
                for at in begins {
                    grew |= time_insert(&mut el.instances, at, el.horizon);
                }
                for at in ends {
                    grew |= time_insert(&mut el.end_instances, at, el.horizon);
                }
            }
            if !grew {
                break;
            }
        }
        for i in 0..self.els.len() {
            let t = self.time_at(self.els[i].svg, self.now_us);
            self.els[i].prune_instances(t);
            prune_times(&mut self.els[i].end_instances, t);
        }
    }

    fn queue_event(&mut self, index: usize, kind: EventKind, iteration: f64) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(QueuedEvent {
                node: self.els[index].node,
                kind,
                iteration: iteration.min(f64::from(i32::MAX)) as i32,
            });
        }
    }

    fn note_progress(&mut self, index: usize, t: f64) {
        let Some(begin) = self.els[index].current_begin(t) else {
            return;
        };
        let el = &self.els[index];
        if !el.reported || (el.reported_begin - begin).abs() > TIME_EPSILON {
            if el.reported && !el.reported_end {
                self.queue_event(index, EventKind::End, 0.0);
            }
            self.queue_event(index, EventKind::Begin, 0.0);
            let el = &mut self.els[index];
            el.reported = true;
            el.reported_end = false;
            el.reported_begin = begin;
            el.reported_iteration = 0.0;
        }
        let el = &self.els[index];
        if el.reported_end {
            return;
        }
        if t >= el.interval_end(begin) {
            self.queue_event(index, EventKind::End, 0.0);
            self.els[index].reported_end = true;
            return;
        }
        if el.dur <= 0.0 {
            return;
        }
        let iteration = ((t - begin) / el.dur).floor();
        if iteration > el.reported_iteration {
            self.els[index].reported_iteration = iteration;
            self.queue_event(index, EventKind::Repeat, iteration);
        }
    }

    fn collect_running(&mut self) -> Vec<Running> {
        let mut running = Vec::new();
        self.active = false;
        for index in 0..self.els.len() {
            let t = self.time_at(self.els[index].svg, self.now_us);
            self.note_progress(index, t);
            let el = &self.els[index];
            if el.target.is_none() || el.attr.is_none() {
                continue;
            }
            let ticking = !self.clock_paused(el.svg);
            if ticking && el.has_future_begin(t) {
                self.active = true;
            }
            let Some(begin) = el.current_begin(t) else {
                continue;
            };
            let end = el.interval_end(begin);
            let ended = t >= end;
            if ended && !el.freeze {
                continue;
            }
            if !ended && ticking {
                self.active = true;
            }
            running.push(Running {
                index,
                begin,
                active_time: if ended { end } else { t } - begin,
                ended,
            });
        }
        running.sort_by(|a, b| {
            a.begin
                .partial_cmp(&b.begin)
                .unwrap_or(Ordering::Equal)
                .then(a.index.cmp(&b.index))
        });
        running
    }

    fn apply_running(&self, r: &Running, table: &mut Overrides) {
        let el = &self.els[r.index];
        let (Some(target), Some(attr)) = (el.target, el.attr.as_deref()) else {
            return;
        };
        let Some(node) = target.get() else {
            return;
        };
        let list = table.entry(target).or_default();
        let slot = list.iter().position(|ov| ov.name.as_c_str() == attr);
        let under = match slot {
            Some(i) => Some(list[i].value.to_string_lossy().into_owned()),
            None if el.kind == Kind::Motion => None,
            None => node.attr(attr).map(|v| v.to_string_lossy().into_owned()),
        };
        let value = values::eval(el, r.active_time, r.ended, under.as_deref())
            .and_then(|value| CString::new(value).ok());
        match (value, slot) {
            (Some(value), Some(i)) => list[i].value = value,
            (Some(value), None) => list.push(Override {
                name: attr.to_owned(),
                value,
            }),
            (None, _) => {
                if list.is_empty() {
                    table.remove(&target);
                }
            }
        }
    }

    fn tick(&mut self, now_us: i64) -> bool {
        self.scan();
        self.now_us = now_us;
        self.dirty = false;
        if self.els.is_empty() && self.overrides.is_empty() {
            self.active = false;
            return false;
        }
        self.resolve_sync();
        let running = self.collect_running();
        let mut next = Overrides::new();
        for r in &running {
            self.apply_running(r, &mut next);
        }
        let changed = self.unreported || next != self.overrides;
        self.unreported = false;
        self.swap_overrides(next);
        changed
    }

    fn dom_event(&mut self, target: Node<'_>, kind: &[u8]) {
        let now = southstar_glib::monotonic_us();
        let doc = self.document();
        let bubbles = event_bubbles(kind);
        let matches = |el: &Element, spec: &Spec| {
            if spec.kind != SpecKind::Event {
                return false;
            }
            let want = if spec.event == b"activate" {
                b"click"
            } else {
                spec.event.as_slice()
            };
            if want != kind {
                return false;
            }
            let base = match spec.id.as_deref() {
                Some(id) => doc.and_then(|doc| index::find_by_id(doc, id)),
                None => el.target.and_then(WeakNode::get),
            };
            event_reaches(base, target, bubbles)
        };
        for i in 0..self.els.len() {
            let t = self.time_at(self.els[i].svg, now);
            let el = &self.els[i];
            let begins: Vec<f64> = el
                .begins
                .iter()
                .filter(|spec| matches(el, spec))
                .map(|spec| t + spec.offset)
                .collect();
            let ends: Vec<f64> = el
                .end_specs
                .iter()
                .filter(|spec| matches(el, spec))
                .map(|spec| t + spec.offset)
                .collect();
            let el = &mut self.els[i];
            for at in begins {
                self.dirty |= el.instance_add(at);
            }
            for at in ends {
                self.dirty |= time_insert(&mut el.end_instances, at, f64::NEG_INFINITY);
            }
        }
    }
}

struct Registration(Rc<RefCell<Timeline>>);

#[derive(Clone)]
pub struct Smil(Rc<Registration>);

impl Smil {
    pub fn new(doc: Node<'_>, now_us: i64) -> Smil {
        let timeline = Rc::new(RefCell::new(Timeline {
            doc: doc.downgrade(),
            els: Vec::new(),
            by_node: HashMap::new(),
            clocks: HashMap::new(),
            overrides: Overrides::new(),
            events: Vec::new(),
            inputs: Inputs::new(),
            origin_us: now_us,
            now_us,
            scanned: false,
            dirty: true,
            active: false,
            unreported: false,
            has_event_specs: false,
        }));
        TIMELINES.with(|all| all.borrow_mut().push(Rc::clone(&timeline)));
        Smil(Rc::new(Registration(timeline)))
    }

    fn timeline(&self) -> &RefCell<Timeline> {
        &self.0.0
    }

    pub fn tick(&self, now_us: i64) -> bool {
        self.timeline().borrow_mut().tick(now_us)
    }

    pub fn origin_us(&self) -> i64 {
        self.timeline().borrow().origin_us
    }

    pub fn has_active(&self) -> bool {
        let timeline = self.timeline().borrow();
        timeline.active || timeline.dirty
    }

    pub fn drain_events(&self, mut deliver: impl FnMut(Node<'static>, EventKind, i32) -> bool) {
        let events = core::mem::take(&mut self.timeline().borrow_mut().events);
        let mut script_ran = false;
        for event in events {
            if script_ran {
                self.timeline().borrow_mut().scan();
            }
            script_ran = false;
            let known = self.timeline().borrow().by_node.contains_key(&event.node);
            if let Some(node) = event.node.get().filter(|_| known) {
                script_ran = deliver(node, event.kind, event.iteration);
            }
        }
    }

    pub fn settle_events(&self) {
        self.drain_events(|node, kind, _| {
            dom_event(node, kind.event_type().to_bytes());
            false
        });
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = TIMELINES.try_with(|all| all.borrow_mut().retain(|t| !Rc::ptr_eq(t, &self.0)));
        if let Ok(mut timeline) = self.0.try_borrow_mut() {
            timeline.set_event_specs(false);
            timeline.swap_overrides(Overrides::new());
        }
    }
}

fn timeline_for(n: Node<'_>) -> Option<Rc<RefCell<Timeline>>> {
    let root = n.root().downgrade();
    TIMELINES.with(|all| {
        all.borrow()
            .iter()
            .find(|t| t.borrow().doc == root)
            .cloned()
    })
}

fn with_timeline<R>(n: Node<'_>, f: impl FnOnce(&mut Timeline) -> R) -> Option<R> {
    let timeline = timeline_for(n)?;
    let mut timeline = timeline.try_borrow_mut().ok()?;
    Some(f(&mut timeline))
}

fn with_element<R>(anim: Node<'_>, f: impl FnOnce(&mut Timeline, usize, f64) -> R) -> Option<R> {
    with_timeline(anim, |timeline| {
        timeline.scan();
        let index = *timeline.by_node.get(&anim.downgrade())?;
        let t = timeline.time_at(timeline.els[index].svg, southstar_glib::monotonic_us());
        Some(f(timeline, index, t))
    })
    .flatten()
}

pub fn any_overrides() -> bool {
    OVERRIDE_NODES.with(Cell::get) != 0
}

pub fn attr(n: Node<'_>, name: &CStr) -> Option<CString> {
    if !any_overrides() {
        return None;
    }
    let key = n.downgrade();
    TIMELINES.with(|all| {
        all.borrow().iter().find_map(|timeline| {
            let timeline = timeline.try_borrow().ok()?;
            let list = timeline.overrides.get(&key)?;
            list.iter()
                .find(|ov| ov.name.as_c_str() == name)
                .map(|ov| ov.value.clone())
        })
    })
}

pub fn node_animated(n: Node<'_>) -> bool {
    if !any_overrides() {
        return false;
    }
    let key = n.downgrade();
    TIMELINES.with(|all| {
        all.borrow().iter().any(|timeline| {
            timeline
                .try_borrow()
                .is_ok_and(|t| t.overrides.contains_key(&key))
        })
    })
}

pub fn refresh(n: Node<'_>) {
    with_timeline(n, |timeline| {
        if timeline.dirty && timeline.tick(southstar_glib::monotonic_us()) {
            timeline.unreported = true;
            timeline.dirty = true;
        }
    });
}

pub fn dom_event(target: Node<'_>, kind: &[u8]) {
    if EVENT_DOCS.with(Cell::get) == 0 {
        return;
    }
    with_timeline(target, |timeline| {
        if timeline.has_event_specs {
            timeline.dom_event(target, kind);
        }
    });
}

pub fn pause(svg: Node<'_>, paused: bool) {
    let Some(outer) = outer_svg(svg) else {
        return;
    };
    with_timeline(svg, |timeline| {
        let clock = timeline.clocks.entry(outer.downgrade()).or_default();
        if clock.paused == paused {
            return;
        }
        let now = southstar_glib::monotonic_us();
        if paused {
            clock.paused_at_us = now;
        } else {
            clock.offset_us += now - clock.paused_at_us;
        }
        clock.paused = paused;
        timeline.dirty = true;
    });
}

pub fn paused(svg: Node<'_>) -> bool {
    with_timeline(svg, |timeline| {
        timeline.clock_paused(outer_svg(svg).map(Node::downgrade))
    })
    .unwrap_or(false)
}

pub fn current_time(el: Node<'_>) -> f64 {
    with_timeline(el, |timeline| {
        timeline.time_at(
            outer_svg(el).map(Node::downgrade),
            southstar_glib::monotonic_us(),
        )
    })
    .unwrap_or(0.0)
}

pub fn set_current_time(svg: Node<'_>, seconds: f64) {
    let Some(outer) = outer_svg(svg).filter(|_| seconds.is_finite()) else {
        return;
    };
    with_timeline(svg, |timeline| {
        let origin = timeline.origin_us;
        let clock = timeline.clocks.entry(outer.downgrade()).or_default();
        let at = if clock.paused {
            clock.paused_at_us
        } else {
            southstar_glib::monotonic_us()
        };
        clock.offset_us = at - origin - (seconds.max(0.0) * 1e6) as i64;
        timeline.dirty = true;
    });
}

pub fn begin_element(anim: Node<'_>, offset: f64) -> bool {
    if !offset.is_finite() {
        return false;
    }
    with_element(anim, |timeline, index, t| {
        timeline.els[index].instance_add(t + offset);
        timeline.dirty = true;
    })
    .is_some()
}

pub fn end_element(anim: Node<'_>, offset: f64) -> bool {
    if !offset.is_finite() {
        return false;
    }
    with_element(anim, |timeline, index, t| {
        let el = &mut timeline.els[index];
        let Some(begin) = el.current_begin(t) else {
            return false;
        };
        el.has_forced_end = true;
        el.forced_begin = begin;
        el.forced_end = t + offset;
        timeline.dirty = true;
        true
    })
    .unwrap_or(false)
}

pub fn start_time(anim: Node<'_>) -> Option<f64> {
    with_element(anim, |timeline, index, t| {
        let el = &timeline.els[index];
        el.current_begin(t)
            .filter(|&begin| t < el.interval_end(begin))
            .or_else(|| el.instances.iter().copied().find(|&b| b > t))
    })
    .flatten()
}

pub fn simple_duration(anim: Node<'_>) -> Option<f64> {
    with_element(anim, |timeline, index, _| timeline.els[index].dur).filter(|&dur| dur >= 0.0)
}

pub fn target(anim: Node<'_>) -> Option<Node<'static>> {
    let doc = timeline_for(anim)?.try_borrow().ok()?.document()?;
    let anim = anim.downgrade().get()?;
    parse::target_of(doc, anim)
}
