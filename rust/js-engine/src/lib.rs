//! Southstar — the engine-neutral JavaScript layer: one API over the in-tree QuickJS-ng fork or Boa, picked by a Cargo feature.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(all(feature = "quickjs", feature = "boa"))]
compile_error!("enable exactly one JavaScript engine: the `quickjs` or the `boa` feature");
#[cfg(not(any(feature = "quickjs", feature = "boa")))]
compile_error!("enable a JavaScript engine: the `quickjs` or the `boa` feature");

#[cfg(feature = "boa")]
mod boa;
#[cfg(feature = "quickjs")]
mod quickjs_ffi;

#[cfg(feature = "boa")]
use boa as backend;
#[cfg(feature = "quickjs")]
use quickjs_ffi as backend;

pub use backend::{ENGINE_NAME, Engine, Scope, Value, engine_version};

pub type NativeFn = for<'a> fn(&mut Scope<'a>, &Value, &[Value]) -> Result<Value, Value>;

pub type RealmInit = for<'a> fn(&mut Scope<'a>) -> Result<(), Value>;

pub enum PromiseState {
    NotAPromise,
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}
