//! Southstar — the editing commands behind document.execCommand and queryCommandSupported/Enabled, and the window's confirm and prompt.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Copy,
    Cut,
    SelectAll,
    Unselect,
}

fn command_arg(scope: &mut Scope<'_>, args: &[Value]) -> Option<Command> {
    let bytes = scope.to_bytes(args.first()?).ok()?;
    let name = bytes.split(|b| *b == 0).next().unwrap_or_default();
    [
        (&b"copy"[..], Command::Copy),
        (b"cut", Command::Cut),
        (b"selectall", Command::SelectAll),
        (b"unselect", Command::Unselect),
    ]
    .into_iter()
    .find(|(known, _)| name.eq_ignore_ascii_case(known))
    .map(|(_, command)| command)
}

fn run(js: Js, command: Command) -> bool {
    match command {
        Command::Copy | Command::Cut => ffi::copy_selection(js),
        Command::SelectAll => ffi::selection_command(js, c"selectAll"),
        Command::Unselect => ffi::selection_command(js, c"unselect"),
    }
}

fn enabled(js: Js, command: Command) -> bool {
    match command {
        Command::Copy | Command::Cut => ffi::selection_copyable(js),
        Command::SelectAll | Command::Unselect => ffi::has_selection_commands(js),
    }
}

pub fn exec_command(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let ok = command_arg(scope, args).is_some_and(|command| !js.is_null() && run(js, command));
    Ok(Value::boolean(ok))
}

pub fn query_command_supported(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(Value::boolean(command_arg(scope, args).is_some()))
}

pub fn query_command_enabled(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let ok = command_arg(scope, args).is_some_and(|command| !js.is_null() && enabled(js, command));
    Ok(Value::boolean(ok))
}

pub(crate) fn confirm(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(Value::boolean(true))
}

pub(crate) fn prompt(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    match args.get(1) {
        Some(default) => Ok(default.clone()),
        None => Ok(scope.string("")),
    }
}
