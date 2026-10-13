//! Internal methods for embedder objects whose property access an embedder hook decides.
//!
//! An embedder installs one [`HostExoticHooks`] with [`set_host_exotic_hooks`] and gives the
//! objects that need it [`host_internal_methods`]`(true)` through [`crate::object::JsData`].
//! The hooks see every own-property lookup, definition, deletion, key enumeration and
//! assignment on those objects; property caches are never filled from them.

use std::sync::OnceLock;

use super::{
    InternalMethodPropertyContext, InternalObjectMethods, ORDINARY_INTERNAL_METHODS,
    ordinary_define_own_property, ordinary_delete, ordinary_get_own_property,
    ordinary_own_property_keys, ordinary_set, validate_and_apply_property_descriptor,
};
use crate::{
    Context, JsResult,
    object::{JsObject, shape::slot::SlotAttributes},
    property::{DescriptorKind, PropertyDescriptor, PropertyKey},
    value::JsValue,
};

/// The embedder's answers to property access on its host objects.
pub trait HostExoticHooks: Sync {
    /// Returns an exotic own property of `object`, or `None` to use the ordinary one.
    ///
    /// `receiver` is the value a `[[Get]]` started from, or `object` itself.
    fn get_own_property(
        &self,
        object: &JsObject,
        key: &PropertyKey,
        receiver: &JsValue,
        context: &mut Context,
    ) -> JsResult<Option<PropertyDescriptor>>;

    /// Defines a property, or returns `None` when the embedder leaves it to the ordinary method.
    fn define_own_property(
        &self,
        object: &JsObject,
        key: &PropertyKey,
        desc: &PropertyDescriptor,
        context: &mut Context,
    ) -> Option<JsResult<bool>>;

    /// Deletes a property, or returns `None` when the embedder leaves it to the ordinary method.
    fn delete(
        &self,
        object: &JsObject,
        key: &PropertyKey,
        context: &mut Context,
    ) -> Option<JsResult<bool>>;

    /// Returns the exotic own keys of `object`; its ordinary keys follow them.
    fn own_property_keys(&self, object: &JsObject, context: &mut Context)
    -> JsResult<Vec<PropertyKey>>;

    /// Assigns a property, or returns `None` when the embedder leaves it to the ordinary method.
    fn set(
        &self,
        object: &JsObject,
        key: &PropertyKey,
        value: &JsValue,
        receiver: &JsValue,
        context: &mut Context,
    ) -> Option<JsResult<bool>>;
}

static HOOKS: OnceLock<&'static dyn HostExoticHooks> = OnceLock::new();

/// Installs the embedder's hooks; only the first call has an effect.
pub fn set_host_exotic_hooks(hooks: &'static dyn HostExoticHooks) {
    let _ = HOOKS.set(hooks);
}

/// The internal methods for a host object: the hooked ones, or the ordinary ones.
#[must_use]
pub fn host_internal_methods(exotic: bool) -> &'static InternalObjectMethods {
    if exotic {
        &HOST_EXOTIC_INTERNAL_METHODS
    } else {
        &ORDINARY_INTERNAL_METHODS
    }
}

/// `OrdinaryDefineOwnProperty` without consulting `object`'s own `[[GetOwnProperty]]`, for
/// hooks that fall back to the ordinary definition.
///
/// # Errors
/// Propagates an error from `[[IsExtensible]]`.
pub fn ordinary_define_own_property_of(
    object: &JsObject,
    key: &PropertyKey,
    desc: PropertyDescriptor,
    context: &mut Context,
) -> JsResult<bool> {
    let context = &mut InternalMethodPropertyContext::new(context);
    let current = ordinary_get_own_property(object, key, context)?;
    let extensible = object.__is_extensible__(context)?;
    Ok(validate_and_apply_property_descriptor(
        Some((object, key)),
        extensible,
        desc,
        current,
        context.slot(),
    ))
}

/// `[[Set]]` on `object` with an explicit `receiver`, so an accessor found on `object` runs
/// with `receiver` as its `this`, for hooks that forward an assignment to a prototype.
///
/// # Errors
/// Propagates an error from the setter or from the receiver's property definition.
pub fn set_with_receiver(
    object: &JsObject,
    key: PropertyKey,
    value: JsValue,
    receiver: JsValue,
    context: &mut Context,
) -> JsResult<bool> {
    let context = &mut InternalMethodPropertyContext::new(context);
    object.__set__(key, value, receiver, context)
}

static HOST_EXOTIC_INTERNAL_METHODS: InternalObjectMethods = InternalObjectMethods {
    __get_own_property__: host_get_own_property,
    __define_own_property__: host_define_own_property,
    __get__: host_get,
    __try_get__: host_try_get,
    __set__: host_set,
    __delete__: host_delete,
    __own_property_keys__: host_own_property_keys,
    ..ORDINARY_INTERNAL_METHODS
};

fn uncacheable(context: &mut InternalMethodPropertyContext<'_>) {
    context.slot().attributes |= SlotAttributes::NOT_CACHEABLE;
}

fn lookup(
    obj: &JsObject,
    key: &PropertyKey,
    receiver: &JsValue,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<Option<PropertyDescriptor>> {
    uncacheable(context);
    if let Some(hooks) = HOOKS.get()
        && let Some(desc) = hooks.get_own_property(obj, key, receiver, context)?
    {
        return Ok(Some(desc));
    }
    ordinary_get_own_property(obj, key, context)
}

fn host_get_own_property(
    obj: &JsObject,
    key: &PropertyKey,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<Option<PropertyDescriptor>> {
    let receiver = JsValue::from(obj.clone());
    lookup(obj, key, &receiver, context)
}

fn read(
    desc: &PropertyDescriptor,
    receiver: &JsValue,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<JsValue> {
    match desc.kind() {
        DescriptorKind::Data {
            value: Some(value), ..
        } => Ok(value.clone()),
        DescriptorKind::Accessor { get: Some(get), .. } if !get.is_undefined() => {
            get.call(receiver, &[], context)
        }
        _ => Ok(JsValue::undefined()),
    }
}

fn host_get(
    obj: &JsObject,
    key: &PropertyKey,
    receiver: JsValue,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<JsValue> {
    let result = match lookup(obj, key, &receiver, context)? {
        Some(desc) => read(&desc, &receiver, context),
        None => match obj.__get_prototype_of__(context)? {
            Some(parent) => parent.__get__(key, receiver, context),
            None => Ok(JsValue::undefined()),
        },
    };
    uncacheable(context);
    result
}

fn host_try_get(
    obj: &JsObject,
    key: &PropertyKey,
    receiver: JsValue,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<Option<JsValue>> {
    let result = match lookup(obj, key, &receiver, context)? {
        Some(desc) => read(&desc, &receiver, context).map(Some),
        None => match obj.__get_prototype_of__(context)? {
            Some(parent) => parent.__try_get__(key, receiver, context),
            None => Ok(None),
        },
    };
    uncacheable(context);
    result
}

fn host_define_own_property(
    obj: &JsObject,
    key: &PropertyKey,
    desc: PropertyDescriptor,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<bool> {
    uncacheable(context);
    if let Some(hooks) = HOOKS.get()
        && let Some(result) = hooks.define_own_property(obj, key, &desc, context)
    {
        return result;
    }
    ordinary_define_own_property(obj, key, desc, context)
}

fn host_set(
    obj: &JsObject,
    key: PropertyKey,
    value: JsValue,
    receiver: JsValue,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<bool> {
    uncacheable(context);
    if let Some(hooks) = HOOKS.get()
        && let Some(result) = hooks.set(obj, &key, &value, &receiver, context)
    {
        return result;
    }
    let result = ordinary_set(obj, key, value, receiver, context);
    uncacheable(context);
    result
}

fn host_delete(
    obj: &JsObject,
    key: &PropertyKey,
    context: &mut InternalMethodPropertyContext<'_>,
) -> JsResult<bool> {
    uncacheable(context);
    if let Some(hooks) = HOOKS.get()
        && let Some(result) = hooks.delete(obj, key, context)
    {
        return result;
    }
    ordinary_delete(obj, key, context)
}

fn host_own_property_keys(obj: &JsObject, context: &mut Context) -> JsResult<Vec<PropertyKey>> {
    let mut keys = match HOOKS.get() {
        Some(hooks) => hooks.own_property_keys(obj, context)?,
        None => Vec::new(),
    };
    for key in ordinary_own_property_keys(obj, context)? {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok(keys)
}
