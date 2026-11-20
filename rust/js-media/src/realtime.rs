//! Southstar — the realtime AudioContext: its running/suspended/closed state under the autoplay policy, a currentTime that follows the wall clock while it runs, and the pump that renders its graph a little ahead and streams it to the audio helper as `pcm` commands.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ops::ControlFlow;
use std::time::Instant;

use southstar_js_engine::{Attributes, ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;
use southstar_webaudio::BLOCK;

use crate::ffi::{self, Js};
use crate::{JsResult, bind_data, dom_exception, get, hooks, rejected, resolved, set, set_str};

/// The rate a realtime context renders at, as its sampleRate reports.
pub(crate) const RATE: f64 = 44100.0;
/// The rate the rendered audio is sent to the helper at (pairs of frames are averaged).
const SEND_RATE: u32 = 22050;
/// How far ahead of the context clock to render, so the helper never runs dry.
pub(crate) const LEAD_S: f64 = 0.12;
/// Past this far behind (a stalled page), skip ahead instead of catching up.
const BEHIND_S: f64 = 0.25;
/// Most audio rendered in one pump.
const CATCHUP_S: f64 = 0.5;
const POLL_MS: u32 = 10;
/// Live contexts per page, like the limit browsers put on them.
const MAX_CONTEXTS: usize = 16;
/// PCM bytes per `pcm` command; base64 keeps the line under the helper's 4 KiB limit.
const LINE_BYTES: usize = 2400;

struct Rt {
    id: u32,
    js: Js,
    realm: ContextHandle,
    obj: Value,
    token: String,
    state: &'static str,
    running: bool,
    want_resume: bool,
    closed: bool,
    statechange: bool,
    started: Instant,
    /// Seconds the context ran before it last started.
    base: f64,
    /// Frames rendered so far: the context time of the next block, times RATE.
    rendered: u64,
    /// The destination had inputs at the last pump.
    audible: bool,
}

thread_local! {
    static CONTEXTS: RefCell<Vec<Rt>> = const { RefCell::new(Vec::new()) };
    /// Pages with a live pump timer, each with the timer's serial.
    static PUMPING: RefCell<Vec<(Js, u32)>> = const { RefCell::new(Vec::new()) };
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
}

impl Rt {
    fn elapsed(&self) -> f64 {
        if self.running {
            self.base + self.started.elapsed().as_secs_f64()
        } else {
            self.base
        }
    }

    fn transition(&mut self, state: &'static str) {
        if self.state != state {
            self.state = state;
            self.statechange = true;
        }
    }

    fn start(&mut self) {
        self.want_resume = false;
        if self.running || self.closed {
            return;
        }
        self.running = true;
        self.audible = true;
        self.started = Instant::now();
        let now = blocks_ceil(self.base * RATE);
        self.rendered = self.rendered.max(now);
        self.transition("running");
    }

    fn stop(&mut self, state: &'static str) {
        if self.running {
            self.base = self.elapsed();
            self.running = false;
        }
        self.transition(state);
    }
}

fn blocks_ceil(frames: f64) -> u64 {
    let block = BLOCK as f64;
    ((frames / block).ceil().max(0.0) * block) as u64
}

fn blocks_floor(frames: f64) -> u64 {
    let block = BLOCK as f64;
    ((frames / block).floor().max(0.0) * block) as u64
}

fn update<R>(id: u32, f: impl FnOnce(&mut Rt) -> R) -> Option<R> {
    CONTEXTS.with(|contexts| {
        contexts
            .borrow_mut()
            .iter_mut()
            .find(|rt| rt.id == id)
            .map(f)
    })
}

fn id_of(scope: &mut Scope<'_>, data: &[Value]) -> u32 {
    data.first()
        .and_then(|id| scope.to_number(id).ok())
        .map_or(0, |id| id as u32)
}

/// True if the autoplay policy lets the page start sound: Southstar has no
/// autoplay setting, so the page must have been activated by the user.
fn allowed(js: Js) -> bool {
    let (transient, ever) = api::user_activation_state(js.ptr());
    transient || ever
}

fn ensure_pump(js: Js) {
    let serial = NEXT_ID.with(|next| {
        let serial = next.get().wrapping_add(1).max(1);
        next.set(serial);
        serial
    });
    let started = PUMPING.with(|pumping| {
        let mut pumping = pumping.borrow_mut();
        if pumping.iter().any(|(page, _)| *page == js) {
            return false;
        }
        pumping.push((js, serial));
        true
    });
    if !started {
        return;
    }
    api::attach_timeout(js.ptr(), POLL_MS, move || {
        // The page may have gone away (teardown drops its entry) since the timer was armed.
        let mine = PUMPING
            .try_with(|pumping| pumping.borrow().contains(&(js, serial)))
            .unwrap_or(false);
        if mine && pump(js) {
            return ControlFlow::Continue(());
        }
        let _ = PUMPING.try_with(|pumping| pumping.borrow_mut().retain(|p| *p != (js, serial)));
        ControlFlow::Break(())
    });
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextet = |shift: u32| char::from(ALPHABET[(n >> shift) as usize & 63]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

/// Sends rendered audio to the helper as s16 mono at SEND_RATE; silence is not sent.
fn emit_pcm(js: Js, token: &str, samples: &[f32]) {
    let mut pcm = Vec::with_capacity(samples.len());
    let mut audible = false;
    for pair in samples.chunks(2) {
        let v = pair.iter().sum::<f32>() / pair.len() as f32;
        let s = (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        audible |= s != 0;
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    if !audible {
        return;
    }
    let hooks = hooks::hooks(js);
    for chunk in pcm.chunks(LINE_BYTES) {
        hooks.emit_audio(&format!("pcm {token} {SEND_RATE} 1 {}", base64(chunk)));
    }
}

struct Work {
    realm: ContextHandle,
    obj: Value,
    token: String,
    t0: f64,
    frames: usize,
}

/// Decides what a running context must render now and claims those frames.
fn plan(rt: &mut Rt) -> Option<Work> {
    if !rt.running || rt.closed {
        return None;
    }
    let now = rt.elapsed() * RATE;
    if (rt.rendered as f64) < now - BEHIND_S * RATE {
        rt.rendered = blocks_floor(now);
    }
    let target = now + LEAD_S * RATE;
    if rt.rendered as f64 >= target {
        return None;
    }
    let frames = blocks_ceil(target - rt.rendered as f64).min(blocks_ceil(CATCHUP_S * RATE));
    let t0 = rt.rendered as f64 / RATE;
    rt.rendered += frames;
    Some(Work {
        realm: rt.realm,
        obj: rt.obj.clone(),
        token: rt.token.clone(),
        t0,
        frames: frames as usize,
    })
}

fn render(work: &Work) -> Option<Vec<f32>> {
    work.realm
        .with(|scope| {
            let destination = get(scope, &work.obj, "destination");
            if !southstar_webaudio::has_inputs(scope, &destination) {
                return None;
            }
            let mut out = Vec::new();
            out.try_reserve_exact(work.frames).ok()?;
            out.resize(work.frames, 0.0f32);
            southstar_webaudio::render_span(scope, &destination, RATE, work.t0, &mut out);
            Some(out)
        })
        .flatten()
}

fn fire_statechange(realm: ContextHandle, obj: &Value, state: &str) {
    realm.with(|scope| {
        set_str(scope, obj, "state", state);
        let event = ffi::new_event(scope);
        set_str(scope, &event, "type", "statechange");
        ffi::dispatch_object(scope, obj, &event);
    });
}

/// One tick of a page's realtime contexts: starts those whose resume waited
/// for user activation, renders the running ones ahead of their clock, and
/// delivers statechange events. Returns false once nothing needs the timer.
fn pump(js: Js) -> bool {
    if api::events_suspended(js.ptr()) {
        return true;
    }
    let ids: Vec<u32> = CONTEXTS.with(|contexts| {
        contexts
            .borrow()
            .iter()
            .filter(|rt| rt.js == js)
            .map(|rt| rt.id)
            .collect()
    });
    let may_start = allowed(js);
    for id in ids {
        let work = update(id, |rt| {
            if rt.want_resume && may_start {
                rt.start();
            }
            plan(rt)
        })
        .flatten();
        let Some(work) = work else {
            continue;
        };
        let rendered = render(&work);
        let live = update(id, |rt| {
            rt.audible = rendered.is_some();
            !rt.closed
        });
        if live == Some(true)
            && let Some(samples) = rendered
        {
            emit_pcm(js, &work.token, &samples);
        }
    }
    let due: Vec<(ContextHandle, Value, &'static str)> = CONTEXTS.with(|contexts| {
        contexts
            .borrow_mut()
            .iter_mut()
            .filter(|rt| rt.js == js && rt.statechange)
            .map(|rt| {
                rt.statechange = false;
                (rt.realm, rt.obj.clone(), rt.state)
            })
            .collect()
    });
    for (realm, obj, state) in &due {
        fire_statechange(*realm, obj, state);
    }
    if !due.is_empty() {
        api::drain_microtasks(js.ptr());
    }
    CONTEXTS.with(|contexts| {
        let mut contexts = contexts.borrow_mut();
        contexts.retain(|rt| !(rt.closed && !rt.statechange));
        contexts
            .iter()
            .any(|rt| rt.js == js && (rt.running || rt.want_resume || rt.statechange))
    })
}

fn current_time(scope: &mut Scope<'_>, this: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let id = id_of(scope, data);
    // Like browsers, report time in whole render quanta.
    let block = BLOCK as f64;
    if let Some(t) = update(id, |rt| {
        (rt.elapsed() * RATE / block).floor() * block / RATE
    }) {
        return Ok(Value::number(t));
    }
    let closed = get(scope, this, "_ct");
    Ok(if closed.is_undefined() {
        Value::number(0.0)
    } else {
        closed
    })
}

fn closed_error(scope: &mut Scope<'_>) -> JsResult {
    let error = dom_exception(scope, "InvalidStateError", "The AudioContext is closed");
    rejected(scope, error)
}

fn resume(scope: &mut Scope<'_>, _: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let id = id_of(scope, data);
    let changed = update(id, |rt| {
        if rt.closed {
            return None;
        }
        if allowed(rt.js) {
            rt.start();
        } else {
            rt.want_resume = true;
        }
        Some((rt.js, rt.obj.clone(), rt.state))
    })
    .flatten();
    let Some((js, obj, state)) = changed else {
        return closed_error(scope);
    };
    set_str(scope, &obj, "state", state);
    ensure_pump(js);
    resolved(scope, Value::undefined())
}

fn suspend(scope: &mut Scope<'_>, _: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let id = id_of(scope, data);
    let changed = update(id, |rt| {
        if rt.closed {
            return None;
        }
        rt.want_resume = false;
        rt.stop("suspended");
        Some((rt.js, rt.obj.clone(), rt.state))
    })
    .flatten();
    let Some((js, obj, state)) = changed else {
        return closed_error(scope);
    };
    set_str(scope, &obj, "state", state);
    ensure_pump(js);
    resolved(scope, Value::undefined())
}

fn close(scope: &mut Scope<'_>, _: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let id = id_of(scope, data);
    let closed = update(id, |rt| {
        if rt.closed {
            return None;
        }
        let at = rt.elapsed();
        rt.want_resume = false;
        rt.stop("closed");
        rt.closed = true;
        Some((rt.js, rt.obj.clone(), rt.token.clone(), at))
    })
    .flatten();
    let Some((js, obj, token, at)) = closed else {
        return closed_error(scope);
    };
    set(scope, &obj, "_ct", Value::number(at));
    set_str(scope, &obj, "state", "closed");
    hooks::hooks(js).emit_audio(&format!("stop {token}"));
    ensure_pump(js);
    resolved(scope, Value::undefined())
}

/// Makes an AudioContext object produce sound: binds resume, suspend, close
/// and a live currentTime, and starts it when the autoplay policy allows.
pub(crate) fn attach(scope: &mut Scope<'_>, context: &Value) -> JsResult<()> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(());
    }
    let live = CONTEXTS.with(|contexts| {
        contexts
            .borrow()
            .iter()
            .filter(|rt| rt.js == js && !rt.closed)
            .count()
    });
    if live >= MAX_CONTEXTS {
        return Err(dom_exception(
            scope,
            "NotSupportedError",
            "Too many AudioContexts are open; close some first",
        ));
    }
    let id = NEXT_ID.with(|next| {
        let id = next.get().wrapping_add(1).max(1);
        next.set(id);
        id
    });
    let data = [Value::number(f64::from(id))];
    let getter = scope.bound_function("get currentTime", 0, current_time, &data);
    scope.define_accessor(
        context,
        "currentTime",
        Some(&getter),
        None,
        Attributes {
            writable: false,
            enumerable: true,
            configurable: true,
        },
    )?;
    bind_data(scope, context, "resume", 0, resume, &data);
    bind_data(scope, context, "suspend", 0, suspend, &data);
    bind_data(scope, context, "close", 0, close, &data);
    set(scope, context, "sampleRate", Value::number(RATE));
    set(scope, context, "outputLatency", Value::number(LEAD_S));
    let mut rt = Rt {
        id,
        js,
        realm: scope.handle(),
        obj: context.clone(),
        token: format!("wa{id}"),
        state: "suspended",
        running: false,
        want_resume: false,
        closed: false,
        statechange: false,
        started: Instant::now(),
        base: 0.0,
        rendered: 0,
        audible: false,
    };
    if allowed(js) {
        rt.start();
        // A context that starts running at once fires no statechange.
        rt.statechange = false;
    }
    set_str(scope, context, "state", rt.state);
    let running = rt.running;
    CONTEXTS.with(|contexts| contexts.borrow_mut().push(rt));
    if running {
        ensure_pump(js);
    }
    Ok(())
}

/// True while a page has a running context with something connected to its
/// destination, so the renderer keeps ticking to feed it.
pub fn busy(page: Option<Page>) -> bool {
    let js = Js::from_ptr(page);
    CONTEXTS
        .try_with(|contexts| {
            contexts.borrow().iter().any(|rt| {
                rt.js == js && ((rt.running && rt.audible && !rt.closed) || rt.statechange)
            })
        })
        .unwrap_or(false)
}

/// Stops every context of a page that is going away.
pub(crate) fn teardown(js: Js) {
    let tokens: Vec<String> = CONTEXTS
        .try_with(|contexts| {
            let mut contexts = contexts.borrow_mut();
            let tokens = contexts
                .iter()
                .filter(|rt| rt.js == js && !rt.closed)
                .map(|rt| rt.token.clone())
                .collect();
            contexts.retain(|rt| rt.js != js);
            tokens
        })
        .unwrap_or_default();
    let hooks = hooks::hooks(js);
    for token in tokens {
        hooks.emit_audio(&format!("stop {token}"));
    }
    let _ = PUMPING.try_with(|pumping| pumping.borrow_mut().retain(|(page, _)| *page != js));
}
