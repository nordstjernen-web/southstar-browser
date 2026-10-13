//! Southstar — who is touching an object on Boa: the browser's Rust natives and engine scripts count as the host, page scripts do not.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;

use boa_engine::Context;
use boa_engine::vm::SourcePath;

pub(crate) const ENGINE_SOURCE_PREFIX: &str = "southstar-engine:";

const IDLE_FRAME_DEPTH: usize = 1;

thread_local! {
    static NATIVE_FRAME_DEPTHS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn in_native<R>(depth: usize, f: impl FnOnce() -> R) -> R {
    NATIVE_FRAME_DEPTHS.with(|depths| depths.borrow_mut().push(depth));
    let result = f();
    NATIVE_FRAME_DEPTHS.with(|depths| depths.borrow_mut().pop());
    result
}

fn frame_is_engine(ctx: &Context, index: usize) -> bool {
    match ctx.frame_source_path(index) {
        Some(SourcePath::Path(path)) => path
            .to_str()
            .is_some_and(|path| path.starts_with(ENGINE_SOURCE_PREFIX)),
        _ => false,
    }
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
