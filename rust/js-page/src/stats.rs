//! Southstar — the page's JavaScript statistics: the engine heap and the event loop's pending work.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::fmt::Write;

use crate::ffi::{self, Js};

pub(crate) fn dump(js: Js) -> String {
    let mut out = String::new();
    if let Some(usage) = js.scope(ffi::memory_usage) {
        out.push_str("JavaScript heap\n");
        let _ = writeln!(out, "  used        {} bytes", usage.memory_used_size);
        let _ = writeln!(out, "  malloc      {} bytes", usage.malloc_size);
        let _ = writeln!(out, "  malloc cap  {} bytes", usage.malloc_limit);
        let _ = writeln!(out, "  objects     {}", usage.obj_count);
        let _ = writeln!(out, "  properties  {}", usage.prop_count);
        let _ = writeln!(out, "  atoms       {}", usage.atom_count);
        let _ = writeln!(out, "  functions   {}", usage.c_func_count);
    }
    let [timers, frames, fetches, xhrs, sockets, listeners] = js.event_loop_counts();
    out.push_str("\nEvent loop\n");
    let _ = writeln!(out, "  timers          {timers}");
    let _ = writeln!(out, "  anim frames     {frames}");
    let _ = writeln!(out, "  pending fetch   {fetches}");
    let _ = writeln!(out, "  pending xhr     {xhrs}");
    let _ = writeln!(out, "  websockets      {sockets}");
    let _ = writeln!(out, "  event listeners {listeners}");
    let _ = writeln!(out, "  frame contexts  {}", js.frame_context_count());
    out
}
