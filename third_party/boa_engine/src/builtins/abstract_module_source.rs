//! Boa's implementation of the `%AbstractModuleSource%` intrinsic.
//!
//! `%AbstractModuleSource%` is the abstract superclass of the module source objects that
//! `import source` and `import.source()` give for modules with a source phase representation.
//! It has no global binding; hosts define its subclasses.
//!
//! More information:
//!  - [Source phase imports proposal][spec]
//!
//! [spec]: https://tc39.es/proposal-source-phase-imports/#sec-module-source-objects

use crate::{
    Context, JsNativeError, JsObject, JsResult, JsString, JsValue,
    builtins::{BuiltInBuilder, BuiltInConstructor, BuiltInObject, IntrinsicObject},
    context::intrinsics::{Intrinsics, StandardConstructor, StandardConstructors},
    js_string,
    property::Attribute,
    realm::Realm,
    symbol::JsSymbol,
};

/// The `%AbstractModuleSource%` constructor and prototype.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AbstractModuleSource;

impl IntrinsicObject for AbstractModuleSource {
    fn init(realm: &Realm) {
        let get_to_string_tag = BuiltInBuilder::callable(realm, Self::get_to_string_tag)
            .name(js_string!("get [Symbol.toStringTag]"))
            .build();

        BuiltInBuilder::from_standard_constructor::<Self>(realm)
            .accessor(
                JsSymbol::to_string_tag(),
                Some(get_to_string_tag),
                None,
                Attribute::CONFIGURABLE | Attribute::NON_ENUMERABLE,
            )
            .build();
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        Self::STANDARD_CONSTRUCTOR(intrinsics.constructors()).constructor()
    }
}

impl BuiltInObject for AbstractModuleSource {
    const NAME: JsString = js_string!("AbstractModuleSource");
}

impl BuiltInConstructor for AbstractModuleSource {
    const CONSTRUCTOR_ARGUMENTS: usize = 0;
    const PROTOTYPE_STORAGE_SLOTS: usize = 2;
    const CONSTRUCTOR_STORAGE_SLOTS: usize = 0;

    const STANDARD_CONSTRUCTOR: fn(&StandardConstructors) -> &StandardConstructor =
        StandardConstructors::abstract_module_source;

    /// [`%AbstractModuleSource% ( )`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-source-phase-imports/#sec-%abstractmodulesource%
    fn constructor(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        // 1. Throw a TypeError exception.
        Err(JsNativeError::typ()
            .with_message("AbstractModuleSource is an abstract class and cannot be constructed")
            .into())
    }
}

impl AbstractModuleSource {
    /// [`get %AbstractModuleSource%.prototype [ %Symbol.toStringTag% ]`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-source-phase-imports/#sec-get-%abstractmodulesource%.prototype.@@tostringtag
    #[allow(clippy::unnecessary_wraps)]
    fn get_to_string_tag(
        this: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, return undefined.
        let Some(object) = this.as_object() else {
            return Ok(JsValue::undefined());
        };

        // 3. Let module be HostGetModuleSourceModuleRecord(O).
        // 4. If module is not-a-source, return undefined.
        let Some(module) = context
            .module_loader()
            .get_module_source_module_record(&object, context)
        else {
            return Ok(JsValue::undefined());
        };

        // 5. Let name be module.GetModuleSourceKind().
        // 6. Assert: name is a String.
        // 7. Return name.
        Ok(module
            .module_source_kind()
            .map_or_else(JsValue::undefined, Into::into))
    }
}
