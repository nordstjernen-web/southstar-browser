//! Southstar — downloads an embedder asks the renderer to fetch: each runs on a thread of its own into a staging file under the cache directory and is polled until it ends.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

const MAX_BYTES: u64 = 1 << 30;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running = 0,
    Done = 1,
    Failed = 2,
}

#[derive(Clone)]
pub struct Status {
    pub state: State,
    pub bytes: u64,
    pub total: u64,
    pub path: Vec<u8>,
    pub final_url: Vec<u8>,
    pub content_type: Vec<u8>,
    pub error: Vec<u8>,
}

struct Job {
    cancelled: AtomicBool,
    path: PathBuf,
    status: Mutex<Status>,
}

static JOBS: Mutex<BTreeMap<i64, Arc<Job>>> = Mutex::new(BTreeMap::new());
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

fn staging_dir() -> Option<PathBuf> {
    let cache = southstar_glib::path_from_bytes(&southstar_glib::user_cache_dir())?;
    let dir = cache.join("southstar").join("downloads");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn create_private(path: &Path) -> Option<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path).ok()
}

fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().into_owned().into_bytes()
}

impl Job {
    fn finish(&self, state: State, update: impl FnOnce(&mut Status)) {
        if let Ok(mut status) = self.status.lock() {
            status.state = state;
            update(&mut status);
        }
        if state == State::Failed || self.cancelled.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn fail(&self, error: &[u8]) {
        self.finish(State::Failed, |status| status.error = error.to_vec());
    }

    fn run(&self, url: &CString, top: Option<&CString>, mut file: File) {
        let Some(resp) =
            southstar_net::download_blocking(url, top.map(CString::as_c_str), MAX_BYTES)
        else {
            return self.fail(b"download failed");
        };
        if self.cancelled.load(Ordering::Relaxed) {
            return self.fail(b"cancelled");
        }
        if let Some(error) = resp.error() {
            return self.fail(error.to_bytes());
        }
        let status = resp.status();
        if status != 0 && !(200..300).contains(&status) {
            return self.fail(b"the server refused the download");
        }
        let body = resp.body().unwrap_or_default();
        if file.write_all(body).and_then(|()| file.flush()).is_err() {
            return self.fail(b"cannot write the download file");
        }
        drop(file);
        let length = body.len() as u64;
        let final_url = resp.final_url().unwrap_or(url).to_bytes().to_vec();
        let content_type = resp
            .content_type()
            .map(|ct| ct.to_bytes().to_vec())
            .unwrap_or_default();
        self.finish(State::Done, |status| {
            status.bytes = length;
            status.total = length;
            status.final_url = final_url;
            status.content_type = content_type;
        });
    }
}

pub fn start(url: &[u8], top: Option<&[u8]>) -> i64 {
    let (Ok(url), Some(dir)) = (CString::new(url), staging_dir()) else {
        return 0;
    };
    if url.is_empty() {
        return 0;
    }
    let top = top.and_then(|t| CString::new(t).ok());
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{}-{id}.part", std::process::id()));
    let Some(file) = create_private(&path) else {
        return 0;
    };
    let job = Arc::new(Job {
        cancelled: AtomicBool::new(false),
        status: Mutex::new(Status {
            state: State::Running,
            bytes: 0,
            total: 0,
            path: path_bytes(&path),
            final_url: Vec::new(),
            content_type: Vec::new(),
            error: Vec::new(),
        }),
        path,
    });
    if let Ok(mut jobs) = JOBS.lock() {
        jobs.insert(id, Arc::clone(&job));
    }
    let worker = Arc::clone(&job);
    let spawned = std::thread::Builder::new()
        .name("ns-download".into())
        .spawn(move || worker.run(&url, top.as_ref(), file));
    if spawned.is_err() {
        job.fail(b"cannot start the download");
    }
    id
}

pub fn poll(id: i64) -> Option<Status> {
    let mut jobs = JOBS.lock().ok()?;
    let status = jobs.get(&id)?.status.lock().ok()?.clone();
    if status.state != State::Running {
        jobs.remove(&id);
    }
    Some(status)
}

pub fn cancel(id: i64) {
    let job = JOBS.lock().ok().and_then(|mut jobs| jobs.remove(&id));
    if let Some(job) = job {
        job.cancelled.store(true, Ordering::Relaxed);
        let running = job
            .status
            .lock()
            .is_ok_and(|status| status.state == State::Running);
        if !running {
            let _ = std::fs::remove_file(&job.path);
        }
    }
}
