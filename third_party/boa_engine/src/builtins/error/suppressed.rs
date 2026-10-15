//! This module implements the global `SuppressedError` object.
//!
//! More information:
//!  - [Proposal reference][spec]
//!
//! [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-suppressederror-objects

use crate::{
    Context, JsArgs, JsResult, JsString, JsValue,
    builtins::{BuiltInBuilder, BuiltInConstructor, BuiltInObject, IntrinsicObject},
    context::intrinsics::{Intrinsics, StandardConstructor, StandardConstructors},
    js_string,
    object::{JsObject, internal_methods::get_prototype_from_constructor},
    property::Attribute,
    realm::Realm,
};

use super::{Error, ErrorKind};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SuppressedError;

impl IntrinsicObject for SuppressedError {
    fn init(realm: &Realm) {
        let attribute = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
        BuiltInBuilder::from_standard_constructor::<Self>(realm)
            .prototype(realm.intrinsics().constructors().error().constructor())
            .inherits(Some(realm.intrinsics().constructors().error().prototype()))
            .property(js_string!("name"), Self::NAME, attribute)
            .property(js_string!("message"), js_string!(), attribute)
            .build();
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        Self::STANDARD_CONSTRUCTOR(intrinsics.constructors()).constructor()
    }
}

impl BuiltInObject for SuppressedError {
    const NAME: JsString = js_string!("SuppressedError");
}

impl BuiltInConstructor for SuppressedError {
    const CONSTRUCTOR_ARGUMENTS: usize = 3;
    const PROTOTYPE_STORAGE_SLOTS: usize = 2;
    const CONSTRUCTOR_STORAGE_SLOTS: usize = 0;

    const STANDARD_CONSTRUCTOR: fn(&StandardConstructors) -> &StandardConstructor =
        StandardConstructors::suppressed_error;

    /// [`SuppressedError ( error, suppressed, message )`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-suppressederror-constructor
    fn constructor(
        new_target: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. If NewTarget is undefined, let newTarget be the active function object; else let newTarget be NewTarget.
        let new_target = &if new_target.is_undefined() {
            context
                .active_function_object()
                .unwrap_or_else(|| {
                    context
                        .intrinsics()
                        .constructors()
                        .suppressed_error()
                        .constructor()
                })
                .into()
        } else {
            new_target.clone()
        };

        // 2. Let O be ? OrdinaryCreateFromConstructor(newTarget, "%SuppressedError.prototype%", « [[ErrorData]] »).
        let prototype = get_prototype_from_constructor(
            new_target,
            StandardConstructors::suppressed_error,
            context,
        )?;
        let o = JsObject::from_proto_and_data_with_shared_shape(
            context.root_shape(),
            prototype,
            Error::with_caller_position(ErrorKind::Error, context),
        )
        .upcast();

        // 3. If message is not undefined, then
        let message = args.get_or_undefined(2);
        if !message.is_undefined() {
            // a. Let msg be ? ToString(message).
            let msg = message.to_string(context)?;

            // b. Perform CreateNonEnumerableDataPropertyOrThrow(O, "message", msg).
            o.create_non_enumerable_data_property_or_throw(js_string!("message"), msg, context);
        }

        // 4. Perform CreateNonEnumerableDataPropertyOrThrow(O, "error", error).
        o.create_non_enumerable_data_property_or_throw(
            js_string!("error"),
            args.get_or_undefined(0).clone(),
            context,
        );

        // 5. Perform CreateNonEnumerableDataPropertyOrThrow(O, "suppressed", suppressed).
        o.create_non_enumerable_data_property_or_throw(
            js_string!("suppressed"),
            args.get_or_undefined(1).clone(),
            context,
        );

        // 6. Return O.
        Ok(o.into())
    }
}

impl SuppressedError {
    /// Creates a `SuppressedError` whose `error` is `error` and whose `suppressed` is
    /// `suppressed`, as `DisposeResources` does when a disposal throws after an earlier throw.
    pub(crate) fn create(error: JsValue, suppressed: JsValue, context: &mut Context) -> JsValue {
        let prototype = context
            .intrinsics()
            .constructors()
            .suppressed_error()
            .prototype();
        let o = JsObject::from_proto_and_data_with_shared_shape(
            context.root_shape(),
            prototype,
            Error::with_caller_position(ErrorKind::Error, context),
        )
        .upcast();
        o.create_non_enumerable_data_property_or_throw(js_string!("error"), error, context);
        o.create_non_enumerable_data_property_or_throw(
            js_string!("suppressed"),
            suppressed,
            context,
        );
        o.into()
    }
}
