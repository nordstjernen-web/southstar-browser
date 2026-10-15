//! Southstar — test262's $262.agent: each agent runs a script in its own thread and engine, and shares SharedArrayBuffers with the main agent.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use southstar_js_engine::{Engine, NativeFn, RuntimeOptions, Scope, SharedMemory, Value};

use crate::host;

const AGENT_STACK_SIZE: usize = 64 * 1024 * 1024;

enum BroadcastId {
    Number(f64),
    BigInt(String),
}

struct Broadcast {
    memory: SharedMemory,
    id: BroadcastId,
    received: Sender<()>,
}

static GENERATION: AtomicU64 = AtomicU64::new(0);
static REPORTS: Mutex<VecDeque<(u64, String)>> = Mutex::new(VecDeque::new());
static EPOCH: OnceLock<Instant> = OnceLock::new();

thread_local! {
    static AGENTS: RefCell<Vec<Sender<Broadcast>>> = const { RefCell::new(Vec::new()) };
    static RECEIVER: RefCell<Option<Value>> = const { RefCell::new(None) };
    static AGENT_GENERATION: RefCell<u64> = const { RefCell::new(0) };
}

pub fn reset() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    AGENTS.with(|agents| agents.borrow_mut().clear());
    if let Ok(mut reports) = REPORTS.lock() {
        reports.clear();
    }
}

fn argument(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

fn monotonic_now(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let epoch = *EPOCH.get_or_init(Instant::now);
    Ok(Value::number(epoch.elapsed().as_secs_f64() * 1000.0))
}

fn sleep(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let ms = scope.to_number(&argument(args, 0))?;
    if ms.is_finite() && ms > 0.0 {
        thread::sleep(Duration::from_secs_f64(ms / 1000.0));
    }
    Ok(Value::undefined())
}

fn start(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let source = scope.to_string(&argument(args, 0))?;
    let generation = GENERATION.load(Ordering::SeqCst);
    let (inbox, messages) = mpsc::channel::<Broadcast>();
    let (ready, running) = mpsc::channel::<()>();
    let spawned = thread::Builder::new()
        .stack_size(AGENT_STACK_SIZE)
        .spawn(move || run_agent(&source, generation, &messages, &ready));
    if spawned.is_err() {
        return Err(scope.type_error("cannot start an agent"));
    }
    if running.recv().is_err() {
        return Err(scope.type_error("the agent stopped before running"));
    }
    AGENTS.with(|agents| agents.borrow_mut().push(inbox));
    Ok(Value::undefined())
}

fn broadcast(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(memory) = scope.shared_memory(&argument(args, 0)) else {
        return Err(scope.type_error("broadcast needs a SharedArrayBuffer"));
    };
    let id_value = argument(args, 1);
    let id = if id_value.is_number() || id_value.is_undefined() {
        BroadcastId::Number(scope.to_number(&id_value)?)
    } else {
        BroadcastId::BigInt(scope.to_string(&id_value)?)
    };
    let (received, acknowledged) = mpsc::channel();
    let sent = AGENTS.with(|agents| {
        agents
            .borrow()
            .iter()
            .filter(|agent| {
                agent
                    .send(Broadcast {
                        memory: memory.clone(),
                        id: match &id {
                            BroadcastId::Number(n) => BroadcastId::Number(*n),
                            BroadcastId::BigInt(s) => BroadcastId::BigInt(s.clone()),
                        },
                        received: received.clone(),
                    })
                    .is_ok()
            })
            .count()
    });
    for _ in 0..sent {
        if acknowledged.recv().is_err() {
            break;
        }
    }
    Ok(Value::undefined())
}

fn get_report(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let generation = GENERATION.load(Ordering::SeqCst);
    let report = REPORTS.lock().ok().and_then(|mut reports| {
        while let Some((report_generation, text)) = reports.pop_front() {
            if report_generation == generation {
                return Some(text);
            }
        }
        None
    });
    Ok(match report {
        Some(text) => scope.string(&text),
        None => Value::null(),
    })
}

fn receive_broadcast(_: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    RECEIVER.with(|receiver| *receiver.borrow_mut() = Some(argument(args, 0)));
    Ok(Value::undefined())
}

fn report(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let text = scope.to_string(&argument(args, 0))?;
    let generation = AGENT_GENERATION.with(|generation| *generation.borrow());
    if let Ok(mut reports) = REPORTS.lock() {
        reports.push_back((generation, text));
    }
    Ok(Value::undefined())
}

fn leaving(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn install_functions(
    scope: &mut Scope<'_>,
    agent: &Value,
    functions: &[(&str, u32, NativeFn)],
) -> Result<(), Value> {
    for &(name, arity, f) in functions {
        let function = scope.function(name, arity, f);
        scope.set(agent, name, function)?;
    }
    Ok(())
}

pub fn install_parent(scope: &mut Scope<'_>, test262: &Value) -> Result<(), Value> {
    let agent = scope.new_object();
    install_functions(
        scope,
        &agent,
        &[
            ("start", 1, start as NativeFn),
            ("broadcast", 2, broadcast),
            ("getReport", 0, get_report),
            ("sleep", 1, sleep),
            ("monotonicNow", 0, monotonic_now),
        ],
    )?;
    scope.set(test262, "agent", agent)
}

fn install_child(scope: &mut Scope<'_>, test262: &Value) -> Result<(), Value> {
    let agent = scope.new_object();
    install_functions(
        scope,
        &agent,
        &[
            ("receiveBroadcast", 1, receive_broadcast as NativeFn),
            ("report", 1, report),
            ("leaving", 0, leaving),
            ("sleep", 1, sleep),
            ("monotonicNow", 0, monotonic_now),
        ],
    )?;
    scope.set(test262, "agent", agent)
}

fn broadcast_id(scope: &mut Scope<'_>, id: &BroadcastId) -> Result<Value, Value> {
    match id {
        BroadcastId::Number(n) => Ok(Value::number(*n)),
        BroadcastId::BigInt(text) => {
            let global = scope.global();
            let constructor = scope.get(&global, "BigInt")?;
            let text = scope.string(text);
            scope.call(&constructor, &Value::undefined(), &[text])
        }
    }
}

fn run_agent(
    source: &str,
    generation: u64,
    messages: &mpsc::Receiver<Broadcast>,
    ready: &Sender<()>,
) {
    AGENT_GENERATION.with(|agent_generation| *agent_generation.borrow_mut() = generation);
    let options = RuntimeOptions {
        can_block: true,
        wait_for_jobs: true,
        ..RuntimeOptions::default()
    };
    let mut engine = Engine::with_options(Path::new("."), &options);
    engine.enter(|scope| {
        let installed = host::install(scope).and_then(|()| {
            let global = scope.global();
            let test262 = scope.get(&global, "$262")?;
            install_child(scope, &test262)
        });
        if installed.is_ok() {
            drop(scope.eval_script(source, "agent"));
            drop(scope.run_jobs());
        }
        let _ = ready.send(());
        while let Ok(message) = messages.recv() {
            let buffer = scope.from_shared_memory(&message.memory);
            let id = broadcast_id(scope, &message.id);
            let _ = message.received.send(());
            let callback = RECEIVER.with(|receiver| receiver.borrow().clone());
            if let (Some(callback), Ok(id)) = (callback, id) {
                drop(scope.call(&callback, &Value::undefined(), &[buffer, id]));
            }
            drop(scope.run_jobs());
        }
    });
}
