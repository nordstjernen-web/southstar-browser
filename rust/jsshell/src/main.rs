//! Southstar — southstar-jsshell: runs scripts, shell benchmarks and test262 on the JavaScript engine the build selected.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod host;
mod test262;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use southstar_js_engine::{Engine, PromiseState, engine_version};

const USAGE: &str = "usage:
  southstar-jsshell --version
  southstar-jsshell run [--module] FILE...
  southstar-jsshell test262 --root DIR [--jobs N] [--timeout-ms N] [--results FILE.jsonl] [--staging] [FILTER...]";

fn run_files(files: &[String], module: bool) -> ExitCode {
    let root = files
        .first()
        .and_then(|f| Path::new(f).parent())
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let mut engine = Engine::new(&root);
    engine.enter(|scope| {
        if let Err(error) = host::install(scope) {
            let (name, message) = host::describe(scope, &error);
            eprintln!("host setup: {name}: {message}");
            return ExitCode::FAILURE;
        }
        for file in files {
            let source = match std::fs::read_to_string(file) {
                Ok(source) => source,
                Err(e) => {
                    eprintln!("{file}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let started = Instant::now();
            let result = if module {
                scope
                    .eval_module(
                        &source,
                        &std::path::absolute(file).unwrap_or(PathBuf::from(file)),
                    )
                    .and_then(|promise| {
                        scope.run_jobs()?;
                        match scope.promise_state(&promise) {
                            PromiseState::Rejected(error) => Err(error),
                            _ => Ok(()),
                        }
                    })
            } else {
                scope
                    .eval_script(&source, file)
                    .and_then(|_| scope.run_jobs())
            };
            let elapsed = started.elapsed();
            if let Err(error) = result {
                let (name, message) = host::describe(scope, &error);
                eprintln!("{file}: uncaught {name}: {message}");
                return ExitCode::FAILURE;
            }
            eprintln!("time {file} {:.3} ms", elapsed.as_secs_f64() * 1000.0);
        }
        ExitCode::SUCCESS
    })
}

const STACK_SIZE: usize = 64 * 1024 * 1024;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || dispatch(args))
        .ok()
        .and_then(|thread| thread.join().ok())
        .unwrap_or(ExitCode::FAILURE)
}

fn dispatch(args: Vec<String>) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("{}", engine_version());
            ExitCode::SUCCESS
        }
        Some("run") => {
            let module = args.iter().any(|a| a == "--module");
            let files: Vec<String> = args[1..]
                .iter()
                .filter(|a| *a != "--module")
                .cloned()
                .collect();
            if files.is_empty() {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
            run_files(&files, module)
        }
        Some("test262-worker") => {
            let root = args.get(1).map(PathBuf::from).unwrap_or_default();
            test262::worker(&root);
            ExitCode::SUCCESS
        }
        Some("test262") => {
            let mut options = test262::Options {
                root: PathBuf::new(),
                filters: Vec::new(),
                jobs: std::thread::available_parallelism().map_or(4, |n| n.get()),
                timeout: Duration::from_secs(10),
                results: None,
                include_staging: false,
            };
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--root" => options.root = rest.next().map(PathBuf::from).unwrap_or_default(),
                    "--jobs" => {
                        options.jobs = rest
                            .next()
                            .and_then(|n| n.parse().ok())
                            .unwrap_or(options.jobs)
                    }
                    "--timeout-ms" => {
                        if let Some(ms) = rest.next().and_then(|n| n.parse().ok()) {
                            options.timeout = Duration::from_millis(ms);
                        }
                    }
                    "--results" => options.results = rest.next().map(PathBuf::from),
                    "--staging" => options.include_staging = true,
                    filter => options.filters.push(filter.to_owned()),
                }
            }
            options.root = std::path::absolute(&options.root).unwrap_or(options.root);
            if !options.root.join("harness").is_dir() {
                eprintln!("southstar-jsshell: --root must point at a test262 checkout\n{USAGE}");
                return ExitCode::FAILURE;
            }
            ExitCode::from(test262::run(options) as u8)
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
