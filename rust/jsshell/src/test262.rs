//! Southstar — a test262 runner over the engine-neutral layer: test metadata, one test in a fresh engine, and a pool of worker processes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{fs, thread};

use southstar_js_engine::{Engine, PromiseState, Scope, Value};

use crate::host;

#[derive(Default)]
struct Meta {
    includes: Vec<String>,
    flags: Vec<String>,
    negative_type: Option<String>,
}

fn list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

fn parse_meta(source: &str) -> Meta {
    let mut meta = Meta::default();
    let Some(start) = source.find("/*---") else {
        return meta;
    };
    let Some(end) = source[start..].find("---*/") else {
        return meta;
    };
    let mut section = "";
    for line in source[start + 5..start + end].lines() {
        let trimmed = line.trim();
        if !line.starts_with(' ') && !line.starts_with('\t') {
            if let Some((key, value)) = trimmed.split_once(':') {
                section = match key.trim() {
                    "includes" => "includes",
                    "flags" => "flags",
                    "negative" => "negative",
                    _ => "",
                };
                match section {
                    "includes" => meta.includes.extend(list(value)),
                    "flags" => meta.flags.extend(list(value)),
                    _ => {}
                }
                continue;
            }
            section = "";
        }
        match section {
            "includes" | "flags" => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    let target = if section == "includes" {
                        &mut meta.includes
                    } else {
                        &mut meta.flags
                    };
                    target.push(item.trim().to_owned());
                }
            }
            "negative" => {
                if let Some(kind) = trimmed.strip_prefix("type:") {
                    meta.negative_type = Some(kind.trim().to_owned());
                }
            }
            _ => {}
        }
    }
    meta
}

pub enum Outcome {
    Pass,
    Fail(String),
}

struct Harness {
    root: PathBuf,
    files: HashMap<String, String>,
}

impl Harness {
    fn source(&mut self, name: &str) -> Result<&str, String> {
        if !self.files.contains_key(name) {
            let path = self.root.join("harness").join(name);
            let text = fs::read_to_string(&path).map_err(|e| format!("harness {name}: {e}"))?;
            self.files.insert(name.to_owned(), text);
        }
        Ok(self.files[name].as_str())
    }
}

fn error_text(scope: &mut Scope<'_>, error: &Value) -> (String, String) {
    host::describe(scope, error)
}

fn expect_error(
    scope: &mut Scope<'_>,
    result: Result<(), Value>,
    negative: Option<&str>,
) -> Outcome {
    match (result, negative) {
        (Ok(()), None) => Outcome::Pass,
        (Ok(()), Some(kind)) => Outcome::Fail(format!("expected {kind}, nothing thrown")),
        (Err(error), Some(kind)) => {
            let (name, message) = error_text(scope, &error);
            if name == kind {
                Outcome::Pass
            } else {
                Outcome::Fail(format!("expected {kind}, got {name}: {message}"))
            }
        }
        (Err(error), None) => {
            let (name, message) = error_text(scope, &error);
            Outcome::Fail(format!("{name}: {message}"))
        }
    }
}

fn run_mode(
    harness: &mut Harness,
    path: &Path,
    source: &str,
    meta: &Meta,
    strict: bool,
) -> Outcome {
    let module = meta.flags.iter().any(|f| f == "module");
    let raw = meta.flags.iter().any(|f| f == "raw");
    let is_async = meta.flags.iter().any(|f| f == "async");
    let mut includes: Vec<String> = Vec::new();
    if !raw {
        includes.push("assert.js".to_owned());
        includes.push("sta.js".to_owned());
        if is_async {
            includes.push("doneprintHandle.js".to_owned());
        }
        includes.extend(meta.includes.iter().cloned());
    }
    let mut harness_sources = Vec::new();
    for include in &includes {
        match harness.source(include) {
            Ok(text) => harness_sources.push((include.clone(), text.to_owned())),
            Err(message) => return Outcome::Fail(message),
        }
    }
    host::take_printed();
    let mut engine = Engine::new(&harness.root);
    engine.enter(|scope| {
        if let Err(error) = host::install(scope) {
            let (name, message) = error_text(scope, &error);
            return Outcome::Fail(format!("host setup: {name}: {message}"));
        }
        for (name, text) in &harness_sources {
            if let Err(error) = scope.eval_script(text, name) {
                let (kind, message) = error_text(scope, &error);
                return Outcome::Fail(format!("harness {name}: {kind}: {message}"));
            }
        }
        let result = if module {
            scope.eval_module(source, path).and_then(|promise| {
                scope.run_jobs()?;
                match scope.promise_state(&promise) {
                    PromiseState::Rejected(error) => Err(error),
                    _ => Ok(()),
                }
            })
        } else {
            let text = if strict {
                format!("\"use strict\";\n{source}")
            } else {
                source.to_owned()
            };
            let name = path.to_string_lossy();
            scope
                .eval_script(&text, &name)
                .and_then(|_| scope.run_jobs())
        };
        let outcome = expect_error(scope, result, meta.negative_type.as_deref());
        if !is_async || !matches!(outcome, Outcome::Pass) || meta.negative_type.is_some() {
            return outcome;
        }
        let printed = host::take_printed();
        if printed
            .iter()
            .any(|line| line == "Test262:AsyncTestComplete")
        {
            Outcome::Pass
        } else if let Some(failure) = printed
            .iter()
            .find(|line| line.starts_with("Test262:AsyncTestFailure"))
        {
            Outcome::Fail(failure.clone())
        } else {
            Outcome::Fail("async test did not complete".to_owned())
        }
    })
}

fn run_test(harness: &mut Harness, path: &Path) -> Outcome {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => return Outcome::Fail(format!("read: {e}")),
    };
    let meta = parse_meta(&source);
    let has = |flag: &str| meta.flags.iter().any(|f| f == flag);
    let modes: &[bool] = if has("module") || has("raw") || has("noStrict") {
        &[false]
    } else if has("onlyStrict") {
        &[true]
    } else {
        &[false, true]
    };
    for &strict in modes {
        if let Outcome::Fail(detail) = run_mode(harness, path, &source, &meta, strict) {
            let mode = if strict { "strict" } else { "sloppy" };
            return Outcome::Fail(format!("[{mode}] {detail}"));
        }
    }
    Outcome::Pass
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
}

pub fn worker(root: &Path) {
    host::set_echo(false);
    let mut harness = Harness {
        root: root.to_path_buf(),
        files: HashMap::new(),
    };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(relative) = line else {
            break;
        };
        let outcome = run_test(&mut harness, &root.join(&relative));
        let reply = match outcome {
            Outcome::Pass => "PASS\t".to_owned(),
            Outcome::Fail(detail) => format!("FAIL\t{}", one_line(&detail)),
        };
        if writeln!(stdout, "{reply}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            break;
        }
    }
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "js")
            && !path.to_string_lossy().contains("_FIXTURE")
        {
            if let Ok(relative) = path.strip_prefix(root) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<String>,
}

fn spawn_worker(exe: &Path, root: &Path) -> Option<Worker> {
    let mut child = Command::new(exe)
        .arg("test262-worker")
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let (send, replies) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if send.send(line).is_err() {
                break;
            }
        }
    });
    Some(Worker {
        child,
        stdin,
        replies,
    })
}

fn group_of(test: &str) -> String {
    let parts: Vec<&str> = test.split('/').collect();
    match parts.as_slice() {
        ["test", top, second, ..] if *top == "built-ins" || *top == "language" => {
            format!("{top}/{second}")
        }
        ["test", top, ..] => (*top).to_owned(),
        _ => "other".to_owned(),
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub struct Options {
    pub root: PathBuf,
    pub filters: Vec<String>,
    pub jobs: usize,
    pub timeout: Duration,
    pub results: Option<PathBuf>,
    pub include_staging: bool,
}

pub fn run(options: Options) -> i32 {
    let test_root = options.root.join("test");
    let mut tests = Vec::new();
    collect(&test_root, &options.root, &mut tests);
    if !options.include_staging {
        tests.retain(|t| !t.starts_with("test/staging/"));
    }
    if !options.filters.is_empty() {
        tests.retain(|t| options.filters.iter().any(|f| t.contains(f.as_str())));
    }
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("southstar-jsshell: cannot find its own executable");
        return 2;
    };
    let total = tests.len();
    eprintln!(
        "{} ({}): {total} tests, {} workers",
        southstar_js_engine::ENGINE_NAME,
        southstar_js_engine::engine_version(),
        options.jobs
    );
    let queue = Arc::new(Mutex::new(tests.into_iter().rev().collect::<Vec<String>>()));
    let results = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let started = Instant::now();
    let mut threads = Vec::new();
    for _ in 0..options.jobs.max(1) {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        let exe = exe.clone();
        let root = options.root.clone();
        let timeout = options.timeout;
        threads.push(thread::spawn(move || {
            let mut worker = spawn_worker(&exe, &root);
            while let Some(test) = queue.lock().ok().and_then(|mut q| q.pop()) {
                let Some(active) = worker.as_mut() else {
                    if let Ok(mut r) = results.lock() {
                        r.push((test, "CRASH".to_owned(), "cannot start worker".to_owned()));
                    }
                    worker = spawn_worker(&exe, &root);
                    continue;
                };
                let sent = writeln!(active.stdin, "{test}").and_then(|()| active.stdin.flush());
                let reply = if sent.is_ok() {
                    active.replies.recv_timeout(timeout)
                } else {
                    Err(RecvTimeoutError::Disconnected)
                };
                let (status, detail) = match reply {
                    Ok(line) => {
                        let (status, detail) = line.split_once('\t').unwrap_or((line.as_str(), ""));
                        (status.to_owned(), detail.to_owned())
                    }
                    Err(RecvTimeoutError::Timeout) => ("TIMEOUT".to_owned(), String::new()),
                    Err(RecvTimeoutError::Disconnected) => ("CRASH".to_owned(), String::new()),
                };
                if status == "TIMEOUT" || status == "CRASH" {
                    if let Some(mut dead) = worker.take() {
                        let _ = dead.child.kill();
                        let _ = dead.child.wait();
                    }
                    worker = spawn_worker(&exe, &root);
                }
                if let Ok(mut r) = results.lock() {
                    r.push((test, status, detail));
                    if r.len() % 2000 == 0 {
                        eprintln!("  {} / {total}", r.len());
                    }
                }
            }
            if let Some(mut done) = worker {
                drop(done.stdin);
                let _ = done.child.wait();
            }
        }));
    }
    for t in threads {
        let _ = t.join();
    }
    let mut results = match Arc::try_unwrap(results) {
        Ok(results) => results.into_inner().unwrap_or_default(),
        Err(shared) => shared.lock().map(|r| r.clone()).unwrap_or_default(),
    };
    results.sort();
    report(&results, started.elapsed(), options.results.as_deref());
    0
}

fn report(results: &[(String, String, String)], elapsed: Duration, path: Option<&Path>) {
    let mut groups: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut tops: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut total = [0usize; 4];
    for (test, status, _) in results {
        let slot = match status.as_str() {
            "PASS" => 0,
            "FAIL" => 1,
            "TIMEOUT" => 2,
            _ => 3,
        };
        let group = group_of(test);
        let top = group.split('/').next().unwrap_or("other").to_owned();
        groups.entry(group).or_default()[slot] += 1;
        tops.entry(top).or_default()[slot] += 1;
        total[slot] += 1;
    }
    let line = |name: &str, c: &[usize; 4]| {
        let n: usize = c.iter().sum();
        let rate = if n == 0 {
            0.0
        } else {
            100.0 * c[0] as f64 / n as f64
        };
        format!(
            "{name:<40} {:>6} / {n:<6} {rate:6.2}%  fail {:<5} timeout {:<4} crash {}",
            c[0], c[1], c[2], c[3]
        )
    };
    println!(
        "# {} ({})",
        southstar_js_engine::ENGINE_NAME,
        southstar_js_engine::engine_version()
    );
    for (name, counts) in &groups {
        println!("{}", line(name, counts));
    }
    println!();
    for (name, counts) in &tops {
        println!("{}", line(name, counts));
    }
    println!("{}", line("total", &total));
    println!("elapsed {:.1}s", elapsed.as_secs_f64());
    if let Some(path) = path {
        let mut out = String::new();
        for (test, status, detail) in results {
            out.push_str(&format!(
                "{{\"test\":{},\"status\":{},\"detail\":{}}}\n",
                json_string(test),
                json_string(status),
                json_string(detail)
            ));
        }
        if let Err(e) = fs::write(path, out) {
            eprintln!("southstar-jsshell: cannot write {}: {e}", path.display());
        }
    }
}
