//! Southstar — conversion of values between JavaScript and WebAssembly, following the ToWebAssemblyValue and ToJSValue rules of the JS API.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use wasmi::{AsContextMut, ExternRef, F32, F64, Func, Nullable, Ref, Val, ValType};

use crate::realm::{HostState, Realm};

pub(crate) enum Raw {
    Val(Val),
    Extern(Option<u32>),
    Func(Option<Func>),
}

pub(crate) fn is_host_compatible(ty: ValType) -> bool {
    matches!(
        ty,
        ValType::I32 | ValType::I64 | ValType::F32 | ValType::F64 | ValType::ExternRef
    )
}

pub(crate) fn to_raw(
    scope: &mut Scope<'_>,
    realm: &Realm,
    ty: ValType,
    value: &Value,
) -> Result<Raw, Value> {
    Ok(match ty {
        ValType::I32 => Raw::Val(Val::I32(scope.to_int32(value)?)),
        ValType::I64 => Raw::Val(Val::I64(scope.to_bigint64(value)?)),
        ValType::F32 => Raw::Val(Val::F32(F32::from_float(scope.to_number(value)? as f32))),
        ValType::F64 => Raw::Val(Val::F64(F64::from_float(scope.to_number(value)?))),
        ValType::ExternRef if value.is_null() => Raw::Extern(None),
        ValType::ExternRef => Raw::Extern(Some(realm.add_extern(value.clone()))),
        ValType::FuncRef if value.is_null() => Raw::Func(None),
        _ => return Err(scope.type_error("unsupported wasm value kind")),
    })
}

pub(crate) fn default_raw(ty: ValType) -> Raw {
    Raw::Val(Val::default_for_ty(ty))
}

impl Raw {
    pub fn into_val(self, ctx: &mut impl AsContextMut<Data = HostState>) -> Val {
        match self {
            Raw::Val(val) => val,
            Raw::Extern(None) => Val::ExternRef(Nullable::Null),
            Raw::Extern(Some(index)) => {
                Val::ExternRef(Nullable::Val(ExternRef::new(ctx.as_context_mut(), index)))
            }
            Raw::Func(None) => Val::FuncRef(Nullable::Null),
            Raw::Func(Some(func)) => Val::FuncRef(Nullable::Val(func)),
        }
    }

    pub fn into_ref(self, ctx: &mut impl AsContextMut<Data = HostState>) -> Option<Ref> {
        match self.into_val(ctx) {
            Val::ExternRef(nullable) => Some(Ref::Extern(nullable)),
            Val::FuncRef(nullable) => Some(Ref::Func(nullable)),
            _ => None,
        }
    }

    pub fn from_val(ctx: &mut impl AsContextMut<Data = HostState>, val: Val) -> Raw {
        match val {
            Val::ExternRef(Nullable::Val(external)) => {
                Raw::Extern(external.data(&*ctx).downcast_ref::<u32>().copied())
            }
            Val::ExternRef(Nullable::Null) => Raw::Extern(None),
            Val::FuncRef(Nullable::Val(func)) => Raw::Func(Some(func)),
            Val::FuncRef(Nullable::Null) => Raw::Func(None),
            other => Raw::Val(other),
        }
    }

    pub fn from_ref(ctx: &mut impl AsContextMut<Data = HostState>, reference: Ref) -> Raw {
        match reference {
            Ref::Extern(nullable) => Raw::from_val(ctx, Val::ExternRef(nullable)),
            Ref::Func(nullable) => Raw::from_val(ctx, Val::FuncRef(nullable)),
        }
    }
}

pub(crate) fn to_js(
    scope: &mut Scope<'_>,
    realm: &Realm,
    realm_obj: Option<&Value>,
    raw: Raw,
) -> Value {
    match raw {
        Raw::Val(Val::I32(number)) => Value::int(number),
        Raw::Val(Val::I64(number)) => scope.bigint64(number),
        Raw::Val(Val::F32(number)) => Value::number(f64::from(number.to_float())),
        Raw::Val(Val::F64(number)) => Value::number(number.to_float()),
        Raw::Val(_) => Value::undefined(),
        Raw::Extern(None) | Raw::Func(None) => Value::null(),
        Raw::Extern(Some(index)) => realm.extern_value(index),
        Raw::Func(Some(func)) => match realm_obj {
            Some(realm_obj) => crate::objects::make_function(scope, realm_obj, func, None, ""),
            None => Value::null(),
        },
    }
}
