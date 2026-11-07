//! Southstar — who is touching an object on Boa, and who made a function: the browser's Rust natives and engine scripts count as the host, page scripts do not.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use boa_engine::builtins::function::OrdinaryFunction;
use boa_engine::native_function::NativeFunctionObject;
use boa_engine::vm::SourcePath;
use boa_engine::{Context, JsObject};

pub(crate) const ENGINE_SOURCE_PREFIX: &str = "southstar-engine:";

const IDLE_FRAME_DEPTH: usize = 1;

thread_local! {
    static NATIVE_FRAME_DEPTHS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static ENGINE_PATHS: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
    static TRUSTED_SOURCES: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

struct TrustedEntry;

impl Drop for TrustedEntry {
    fn drop(&mut self) {
        TRUSTED_SOURCES.with(|names| names.borrow_mut().pop());
    }
}

pub(crate) fn with_trusted_source<R>(name: &str, f: impl FnOnce() -> R) -> R {
    TRUSTED_SOURCES.with(|names| names.borrow_mut().push(PathBuf::from(name)));
    let _entry = TrustedEntry;
    f()
}

pub(crate) fn stack_is_trusted(ctx: &Context) -> bool {
    let Some(name) = TRUSTED_SOURCES.with(|names| names.borrow().last().cloned()) else {
        return false;
    };
    let depth = ctx.frame_depth();
    let mut saw_source = false;
    for index in IDLE_FRAME_DEPTH..depth {
        match ctx.frame_source_path(index) {
            Some(SourcePath::Path(path)) if *path == name => saw_source = true,
            Some(SourcePath::Eval) if saw_source => {}
            _ => return false,
        }
    }
    saw_source
}

pub(crate) struct HostEntry;

impl Drop for HostEntry {
    fn drop(&mut self) {
        NATIVE_FRAME_DEPTHS.with(|depths| depths.borrow_mut().pop());
    }
}

pub(crate) fn enter(ctx: &Context) -> HostEntry {
    let depth = ctx.frame_depth();
    NATIVE_FRAME_DEPTHS.with(|depths| depths.borrow_mut().push(depth));
    HostEntry
}

pub(crate) fn in_native<R>(depth: usize, f: impl FnOnce() -> R) -> R {
    NATIVE_FRAME_DEPTHS.with(|depths| depths.borrow_mut().push(depth));
    let _entry = HostEntry;
    f()
}

pub(crate) fn mark_engine_path(path: &Path) {
    ENGINE_PATHS.with(|paths| {
        paths.borrow_mut().insert(path.to_path_buf());
    });
}

fn is_engine_path(path: &SourcePath) -> bool {
    match path {
        SourcePath::Path(path) => {
            path.to_str()
                .is_some_and(|path| path.starts_with(ENGINE_SOURCE_PREFIX))
                || ENGINE_PATHS.with(|paths| paths.borrow().contains(&**path))
        }
        _ => false,
    }
}

fn frame_is_engine(ctx: &Context, index: usize) -> bool {
    ctx.frame_source_path(index)
        .is_some_and(|path| is_engine_path(&path))
}

fn host_above(ctx: &Context, native_depths: &[usize], depth: usize) -> bool {
    let floor = native_depths.last().copied().unwrap_or(IDLE_FRAME_DEPTH);
    depth <= floor || frame_is_engine(ctx, depth - 1)
}

pub(crate) fn is_host_access(ctx: &Context) -> bool {
    NATIVE_FRAME_DEPTHS.with(|depths| host_above(ctx, &depths.borrow(), ctx.frame_depth()))
}

pub(crate) fn is_host_caller(ctx: &Context) -> bool {
    NATIVE_FRAME_DEPTHS.with(|depths| {
        let depths = depths.borrow();
        match depths.split_last() {
            Some((&native, outer)) => host_above(ctx, outer, native),
            None => host_above(ctx, &[], ctx.frame_depth()),
        }
    })
}

pub(crate) fn is_engine_function(object: &JsObject) -> bool {
    if let Some(native) = object.downcast_ref::<NativeFunctionObject>() {
        return native.is_host();
    }
    object
        .downcast_ref::<OrdinaryFunction>()
        .is_some_and(|function| is_engine_path(function.codeblock().path()))
}
