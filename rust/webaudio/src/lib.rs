//! Southstar — Web Audio rendering: walks the AudioNode graph of an AudioContext or OfflineAudioContext in 128-frame blocks, with AudioParam automation and per-node state carried between blocks, and mixes it into one channel.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod automation;

use core::f64::consts::PI;
use core::sync::atomic::{AtomicU64, Ordering};

use southstar_js_engine::props::{get, set};
use southstar_js_engine::{ElementType, Scope, Value};

pub use automation::{Event, cancel, evaluate, insert, param_at, param_method};

/// Frames in one render quantum.
pub const BLOCK: usize = 128;
const MAX_DEPTH: i32 = 32;
const MAX_NODE_RENDERS: u32 = 4096;
const MAX_INPUT_VISITS: u32 = 1 << 16;
const MAX_DELAY_S: f64 = 30.0;

struct Walk {
    depth: i32,
    renders_left: u32,
    visits_left: u32,
    /// Unique id of the block being rendered; per-node state uses it.
    block: f64,
    /// Context time of the block's first frame.
    t0: f64,
}

fn positive(x: f64) -> bool {
    x > 0.0
}

fn non_negative(x: f64) -> bool {
    x >= 0.0
}

fn next_block() -> f64 {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    (SERIAL.fetch_add(1, Ordering::Relaxed) + 1) as f64
}

fn number(scope: &mut Scope, object: &Value, name: &str, default: f64) -> f64 {
    match scope.get(object, name) {
        Ok(value) if !value.is_undefined() && !value.is_null() => {
            scope.to_number(&value).unwrap_or(default)
        }
        _ => default,
    }
}

fn string(scope: &mut Scope, object: &Value, name: &str) -> Vec<u8> {
    let value = get(scope, object, name);
    if !value.is_string() {
        return Vec::new();
    }
    let mut text = scope.to_bytes(&value).unwrap_or_default();
    if let Some(nul) = text.iter().position(|&b| b == 0) {
        text.truncate(nul);
    }
    text
}

fn new_typed(scope: &mut Scope, constructor: &str, length: usize) -> Option<Value> {
    let global = scope.global();
    let constructor = get(scope, &global, constructor);
    if !constructor.is_object() {
        return None;
    }
    scope
        .construct(&constructor, &[Value::number(length as f64)])
        .ok()
}

fn float32_len(scope: &mut Scope, array: &Value) -> Option<usize> {
    if !array.is_object() || scope.typed_array_element(array) != Some(ElementType::Float32) {
        return None;
    }
    scope.with_typed_array(array, |view| view.bytes.len() / 4)
}

struct Samples<'a>(&'a [u8]);

impl Samples<'_> {
    fn len(&self) -> usize {
        self.0.len() / 4
    }

    fn get(&self, index: usize) -> f32 {
        let at = index * 4;
        f32::from_ne_bytes([self.0[at], self.0[at + 1], self.0[at + 2], self.0[at + 3]])
    }
}

fn with_float32<R>(scope: &mut Scope, array: &Value, f: impl FnOnce(&Samples) -> R) -> Option<R> {
    if !array.is_object() || scope.typed_array_element(array) != Some(ElementType::Float32) {
        return None;
    }
    scope.with_typed_array(array, |view| f(&Samples(view.bytes)))
}

/// Per-node state that survives between blocks, kept in `node._st` as a
/// Float64Array of 2n doubles: the state at the block's start, then at its end.
/// A node rendered twice in one block (fan-out) starts both times from the
/// block-start state, so it advances once.
struct State {
    array: Value,
    values: Vec<f64>,
}

fn state_load(scope: &mut Scope, node: &Value, n: usize, walk: &Walk) -> Option<State> {
    let mut array = get(scope, node, "_st");
    let fits = scope.typed_array_element(&array) == Some(ElementType::Float64)
        && scope.with_typed_array(&array, |view| view.bytes.len()) == Some(16 * n);
    if !fits {
        array = new_typed(scope, "Float64Array", 2 * n)?;
        set(scope, node, "_st", array.clone());
    }
    let new_block = number(scope, node, "_sblk", -1.0) != walk.block;
    if new_block {
        set(scope, node, "_sblk", Value::number(walk.block));
    }
    let all: Vec<f64> = scope.with_typed_array(&array, |view| {
        view.bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|b| f64::from_ne_bytes(*b))
            .collect()
    })?;
    if all.len() != 2 * n {
        return None;
    }
    let values = if new_block {
        all[n..].to_vec()
    } else {
        all[..n].to_vec()
    };
    if new_block {
        state_write(scope, &array, 0, &values);
    }
    Some(State { array, values })
}

fn state_write(scope: &mut Scope, array: &Value, at: usize, values: &[f64]) {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
    scope.with_buffer_bytes_mut(array, |out| {
        if let Some(dst) = out.get_mut(at * 8..at * 8 + bytes.len()) {
            dst.copy_from_slice(&bytes);
        }
    });
}

impl State {
    fn save(&self, scope: &mut Scope) {
        state_write(scope, &self.array, self.values.len(), &self.values);
    }
}

/// Converts a source node's start/stop times into a frame range of this block.
/// A source that was never started plays nothing. Returns `None` once the
/// source has stopped for good, so its output can be dropped from the graph.
fn window(
    scope: &mut Scope,
    node: &Value,
    frames: usize,
    rate: f64,
    walk: &Walk,
) -> Option<(usize, usize)> {
    let start = number(scope, node, "_startTime", -1.0);
    let stop = number(scope, node, "_stopTime", -1.0);
    if !non_negative(start) {
        return Some((0, 0));
    }
    let t0 = walk.t0;
    let t1 = t0 + frames as f64 / rate;
    if stop >= 0.0 && stop <= t0 {
        return None;
    }
    let f = (start - t0) * rate;
    let first = if f <= 0.0 {
        0
    } else if start >= t1 {
        frames
    } else {
        (f.ceil() as usize).min(frames)
    };
    let mut last = frames;
    if stop >= 0.0 && stop < t1 {
        let e = (stop - t0) * rate;
        last = if e <= 0.0 {
            0
        } else {
            (e.ceil() as usize).min(frames)
        };
    }
    Some((first, last.max(first)))
}

fn oscillator(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &Walk) -> bool {
    let Some((first, last)) = window(scope, node, out.len(), rate, walk) else {
        return true;
    };
    if first >= last {
        return false;
    }
    let frequency = param_at(scope, node, "frequency", 440.0, walk.t0);
    let detune = param_at(scope, node, "detune", 0.0, walk.t0);
    let mut f = frequency * 2.0f64.powf(detune / 1200.0);
    if !positive(f) {
        f = 0.0;
    }
    f = f.min(rate * 0.5);
    let wave = string(scope, node, "type");
    let Some(mut state) = state_load(scope, node, 1, walk) else {
        return false;
    };
    let step = f / rate;
    let mut phase = state.values[0];
    if !(0.0..1.0).contains(&phase) {
        phase = 0.0;
    }
    for sample in &mut out[first..last] {
        let v = match wave.as_slice() {
            b"square" => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            b"sawtooth" => 2.0 * phase - 1.0,
            b"triangle" => {
                if phase < 0.5 {
                    4.0 * phase - 1.0
                } else {
                    3.0 - 4.0 * phase
                }
            }
            _ => (2.0 * PI * phase).sin(),
        };
        *sample = v as f32;
        phase += step;
        if phase >= 1.0 {
            phase -= phase.floor();
        }
    }
    state.values[0] = phase;
    state.save(scope);
    false
}

fn buffer_source(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &Walk) -> bool {
    let Some((first, last)) = window(scope, node, out.len(), rate, walk) else {
        return true;
    };
    if first >= last {
        return false;
    }
    let buffer = get(scope, node, "buffer");
    if !buffer.is_object() {
        return false;
    }
    let mut ratio = param_at(scope, node, "playbackRate", 1.0, walk.t0);
    let buffer_rate = number(scope, &buffer, "sampleRate", rate);
    if !positive(ratio) {
        ratio = 1.0;
    }
    let mut step = ratio
        * if buffer_rate > 0.0 {
            buffer_rate / rate
        } else {
            1.0
        };
    if !step.is_finite() {
        step = 1.0;
    }
    let looping = match scope.get(node, "loop") {
        Ok(value) => scope.to_bool(&value),
        Err(_) => false,
    };
    let chans = get(scope, &buffer, "_chans");
    if !chans.is_object() {
        return false;
    }
    let channel = scope
        .get_index(&chans, 0)
        .unwrap_or_else(|_| Value::undefined());
    let Some(mut state) = state_load(scope, node, 1, walk) else {
        return false;
    };
    let mut pos = state.values[0];
    if !non_negative(pos) {
        pos = 0.0;
    }
    let done = with_float32(scope, &channel, |samples| {
        if samples.len() == 0 {
            return false;
        }
        let n = samples.len() as f64;
        for sample in &mut out[first..last] {
            if pos >= n {
                if !looping {
                    return true;
                }
                pos %= n;
            }
            *sample = samples.get(pos as usize);
            pos += step;
        }
        !looping && pos >= n
    })
    .unwrap_or(false);
    state.values[0] = pos;
    state.save(scope);
    done
}

fn constant(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &Walk) -> bool {
    let Some((first, last)) = window(scope, node, out.len(), rate, walk) else {
        return true;
    };
    if first < last {
        let v = param_at(scope, node, "offset", 1.0, walk.t0);
        out[first..last].fill(v as f32);
    }
    false
}

fn input_count(scope: &mut Scope, inputs: &Value) -> u32 {
    let count = number(scope, inputs, "length", 0.0);
    if count > 0.0 {
        count.min(f64::from(u32::MAX)) as u32
    } else {
        0
    }
}

/// Renders each connected input and mixes them into `out`. Sources that have
/// finished for good are dropped from the inputs, as a browser would collect
/// them, so long-running pages do not keep rendering dead notes.
fn sum_inputs(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &mut Walk) {
    let inputs = get(scope, node, "_inputs");
    if !inputs.is_object() {
        return;
    }
    let count = input_count(scope, &inputs);
    let mut scratch = [0.0f32; BLOCK];
    let scratch = &mut scratch[..out.len().min(BLOCK)];
    let mut finished = Vec::new();
    walk.depth += 1;
    let mut index = 0;
    while index < count && walk.renders_left > 0 && walk.visits_left > 0 {
        walk.visits_left -= 1;
        let source = scope
            .get_index(&inputs, index)
            .unwrap_or_else(|_| Value::undefined());
        if source.is_object() {
            scratch.fill(0.0);
            if render(scope, &source, rate, scratch, walk) {
                finished.push(source);
            }
            for (sample, input) in out.iter_mut().zip(scratch.iter()) {
                *sample += input;
            }
        }
        index += 1;
    }
    walk.depth -= 1;
    if finished.is_empty() {
        return;
    }
    let inputs = get(scope, node, "_inputs");
    if !inputs.is_object() {
        return;
    }
    let count = input_count(scope, &inputs);
    let kept = scope.new_array();
    let mut next = 0;
    for index in 0..count {
        let source = scope
            .get_index(&inputs, index)
            .unwrap_or_else(|_| Value::undefined());
        if !finished.iter().any(|done| done.same_object(&source)) {
            let _ = scope.set_index(&kept, next, source);
            next += 1;
        }
    }
    set(scope, node, "_inputs", kept);
}

fn compressor(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &Walk) {
    let t0 = walk.t0;
    let threshold = param_at(scope, node, "threshold", -24.0, t0);
    let knee = param_at(scope, node, "knee", 30.0, t0).max(0.0);
    let ratio = param_at(scope, node, "ratio", 12.0, t0).max(1.0);
    let attack = param_at(scope, node, "attack", 0.003, t0);
    let release = param_at(scope, node, "release", 0.25, t0);
    let atk = if attack > 0.0 {
        (-1.0 / (attack * rate)).exp()
    } else {
        0.0
    };
    let rel = if release > 0.0 {
        (-1.0 / (release * rate)).exp()
    } else {
        0.0
    };
    let Some(mut state) = state_load(scope, node, 1, walk) else {
        return;
    };
    let mut env = if state.values[0].is_finite() {
        state.values[0]
    } else {
        0.0
    };
    for sample in out.iter_mut() {
        let x = f64::from(*sample).abs();
        let db = if x > 1e-9 { 20.0 * x.log10() } else { -180.0 };
        let over = db - threshold;
        let reduction = if knee > 0.0 && over > -knee * 0.5 && over < knee * 0.5 {
            let t = over + knee * 0.5;
            (1.0 / ratio - 1.0) * t * t / (2.0 * knee)
        } else if over <= 0.0 {
            0.0
        } else {
            over * (1.0 / ratio - 1.0)
        };
        let coeff = if reduction < env { atk } else { rel };
        env = coeff * env + (1.0 - coeff) * reduction;
        *sample = (f64::from(*sample) * 10.0f64.powf(env / 20.0)) as f32;
    }
    state.values[0] = env;
    state.save(scope);
}

fn biquad(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &Walk) {
    let t0 = walk.t0;
    let mut f0 = param_at(scope, node, "frequency", 350.0, t0);
    let mut q = param_at(scope, node, "Q", 1.0, t0);
    let gain_db = param_at(scope, node, "gain", 0.0, t0);
    let kind = string(scope, node, "type");
    if !positive(f0) {
        f0 = 1.0;
    }
    f0 = f0.min(rate * 0.5);
    if !positive(q) {
        q = 1e-4;
    }
    let w0 = 2.0 * PI * f0 / rate;
    let (cw, sw) = (w0.cos(), w0.sin());
    let alpha = sw / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = match kind.as_slice() {
        b"highpass" => (
            (1.0 + cw) / 2.0,
            -(1.0 + cw),
            (1.0 + cw) / 2.0,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        ),
        b"bandpass" => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
        b"notch" => (1.0, -2.0 * cw, 1.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
        b"allpass" => (
            1.0 - alpha,
            -2.0 * cw,
            1.0 + alpha,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        ),
        b"peaking" => {
            let a = 10.0f64.powf(gain_db / 40.0);
            (
                1.0 + alpha * a,
                -2.0 * cw,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cw,
                1.0 - alpha / a,
            )
        }
        _ => (
            (1.0 - cw) / 2.0,
            1.0 - cw,
            (1.0 - cw) / 2.0,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        ),
    };
    let Some(mut state) = state_load(scope, node, 4, walk) else {
        return;
    };
    let [mut x1, mut x2, mut y1, mut y2] = [0, 1, 2, 3].map(|i| state.values[i]);
    if !(x1 + x2 + y1 + y2).is_finite() {
        (x1, x2, y1, y2) = (0.0, 0.0, 0.0, 0.0);
    }
    for sample in out.iter_mut() {
        let x = f64::from(*sample);
        let y = (b0 / a0) * x + (b1 / a0) * x1 + (b2 / a0) * x2 - (a1 / a0) * y1 - (a2 / a0) * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
        *sample = y as f32;
    }
    state.values.copy_from_slice(&[x1, x2, y1, y2]);
    state.save(scope);
}

/// Delays the signal by delayTime through a ring buffer kept on the node.
fn delay(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &Walk) {
    let mut max_t = number(scope, node, "_maxDelay", 1.0);
    if !positive(max_t) {
        max_t = 1.0;
    }
    max_t = max_t.min(MAX_DELAY_S);
    let mut t = param_at(scope, node, "delayTime", 0.0, walk.t0);
    if !positive(t) {
        t = 0.0;
    }
    t = t.min(max_t);
    let size = (max_t * rate).ceil() as usize + BLOCK + 1;
    let mut ring = get(scope, node, "_ring");
    if float32_len(scope, &ring) != Some(size) {
        let Some(fresh) = new_typed(scope, "Float32Array", size) else {
            return;
        };
        ring = fresh;
        set(scope, node, "_ring", ring.clone());
    }
    let Some(mut state) = state_load(scope, node, 1, walk) else {
        return;
    };
    let d = (t * rate) as usize;
    let mut w = (state.values[0].max(0.0) as usize) % size;
    let done = scope.with_buffer_bytes_mut(&ring, |bytes| {
        if bytes.len() != size * 4 {
            return false;
        }
        for sample in out.iter_mut() {
            bytes[w * 4..w * 4 + 4].copy_from_slice(&sample.to_ne_bytes());
            let r = (w + size - d) % size;
            let mut b = [0u8; 4];
            b.copy_from_slice(&bytes[r * 4..r * 4 + 4]);
            *sample = f32::from_ne_bytes(b);
            w = (w + 1) % size;
        }
        true
    });
    if done == Some(true) {
        state.values[0] = w as f64;
        state.save(scope);
    }
}

fn waveshaper(scope: &mut Scope, node: &Value, out: &mut [f32]) {
    let curve = get(scope, node, "curve");
    with_float32(scope, &curve, |curve| {
        let n = curve.len();
        if n < 2 {
            return;
        }
        for sample in out.iter_mut() {
            let x = f64::from(*sample).clamp(-1.0, 1.0);
            let pos = (x + 1.0) * 0.5 * (n - 1) as f64;
            let k = pos as usize;
            if k >= n - 1 {
                *sample = curve.get(n - 1);
                continue;
            }
            let frac = pos - k as f64;
            *sample = (f64::from(curve.get(k)) * (1.0 - frac) + f64::from(curve.get(k + 1)) * frac)
                as f32;
        }
    });
}

fn scale(out: &mut [f32], gain: f64) {
    for sample in out.iter_mut() {
        *sample = (f64::from(*sample) * gain) as f32;
    }
}

/// Renders one block (at most `BLOCK` frames) of a node by its `_kind`.
/// Returns true when the node is a source that has finished for good.
fn render(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32], walk: &mut Walk) -> bool {
    if walk.depth > MAX_DEPTH || walk.renders_left == 0 || !node.is_object() {
        return false;
    }
    walk.renders_left -= 1;
    let kind = string(scope, node, "_kind");
    match kind.as_slice() {
        b"oscillator" => return oscillator(scope, node, rate, out, walk),
        b"buffersource" => return buffer_source(scope, node, rate, out, walk),
        b"constant" => return constant(scope, node, rate, out, walk),
        _ => {}
    }
    sum_inputs(scope, node, rate, out, walk);
    let t0 = walk.t0;
    match kind.as_slice() {
        b"gain" => {
            let gain = param_at(scope, node, "gain", 1.0, t0);
            scale(out, gain);
        }
        b"compressor" => compressor(scope, node, rate, out, walk),
        b"biquad" => biquad(scope, node, rate, out, walk),
        b"delay" => delay(scope, node, rate, out, walk),
        b"waveshaper" => waveshaper(scope, node, out),
        b"stereopanner" | b"panner" => {
            let gain = 1.0 - param_at(scope, node, "pan", 0.0, t0).abs() * 0.5;
            scale(out, gain);
        }
        _ => {}
    }
    false
}

/// Renders `out.len()` frames of the graph feeding `destination`, starting at
/// context time `t0`, block by block, clamped to [-1, 1].
pub fn render_span(scope: &mut Scope, destination: &Value, rate: f64, t0: f64, out: &mut [f32]) {
    out.fill(0.0);
    if !destination.is_object() || !positive(rate) {
        return;
    }
    for (index, block) in out.chunks_mut(BLOCK).enumerate() {
        let mut walk = Walk {
            depth: 0,
            renders_left: MAX_NODE_RENDERS,
            visits_left: MAX_INPUT_VISITS,
            block: next_block(),
            t0: t0 + (index * BLOCK) as f64 / rate,
        };
        render(scope, destination, rate, block, &mut walk);
    }
    for sample in out.iter_mut() {
        *sample = if sample.is_nan() {
            0.0
        } else {
            sample.clamp(-1.0, 1.0)
        };
    }
}

/// True if any node is connected to `destination`.
pub fn has_inputs(scope: &mut Scope, destination: &Value) -> bool {
    let inputs = get(scope, destination, "_inputs");
    inputs.is_object() && number(scope, &inputs, "length", 0.0) > 0.0
}

/// Renders an OfflineAudioContext's graph into `out` from time zero.
pub fn render_offline(scope: &mut Scope, destination: &Value, rate: f64, out: &mut [f32]) -> bool {
    out.fill(0.0);
    if !destination.is_object() {
        return false;
    }
    render_span(scope, destination, rate, 0.0, out);
    true
}
