//! Southstar — declarative property tables: the native methods and accessors of a prototype or namespace, written once as a static slice and installed by either engine.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{NativeFn, Scope, Value};

pub type GetterFn = for<'a> fn(&mut Scope<'a>, &Value) -> Result<Value, Value>;

pub type SetterFn = for<'a> fn(&mut Scope<'a>, &Value, &Value) -> Result<(), Value>;

pub type GetterMagicFn = for<'a> fn(&mut Scope<'a>, &Value, i32) -> Result<Value, Value>;

pub type SetterMagicFn = for<'a> fn(&mut Scope<'a>, &Value, &Value, i32) -> Result<(), Value>;

#[derive(Clone, Copy)]
pub enum Prop {
    Method {
        name: &'static str,
        arity: u8,
        f: NativeFn,
        enumerable: bool,
    },
    Accessor {
        name: &'static str,
        get: Option<GetterFn>,
        set: Option<SetterFn>,
    },
    AccessorMagic {
        name: &'static str,
        get: Option<GetterMagicFn>,
        set: Option<SetterMagicFn>,
        magic: i16,
    },
}

impl Prop {
    pub fn name(&self) -> &'static str {
        match *self {
            Prop::Method { name, .. }
            | Prop::Accessor { name, .. }
            | Prop::AccessorMagic { name, .. } => name,
        }
    }
}

pub const fn method(name: &'static str, arity: u8, f: NativeFn) -> Prop {
    Prop::Method {
        name,
        arity,
        f,
        enumerable: true,
    }
}

pub const fn method_enumerable(
    name: &'static str,
    arity: u8,
    f: NativeFn,
    enumerable: bool,
) -> Prop {
    Prop::Method {
        name,
        arity,
        f,
        enumerable,
    }
}

pub const fn getter(name: &'static str, get: GetterFn) -> Prop {
    Prop::Accessor {
        name,
        get: Some(get),
        set: None,
    }
}

pub const fn accessor(name: &'static str, get: Option<GetterFn>, set: Option<SetterFn>) -> Prop {
    Prop::Accessor { name, get, set }
}

pub const fn accessor_magic(
    name: &'static str,
    get: Option<GetterMagicFn>,
    set: Option<SetterMagicFn>,
    magic: i16,
) -> Prop {
    Prop::AccessorMagic {
        name,
        get,
        set,
        magic,
    }
}
