//! Boa's implementation of the `ShadowRealm` built-in object.
//!
//! A `ShadowRealm` runs code in a new realm with its own intrinsics and global object. Only
//! primitives and callables cross the boundary between the realms, and callables cross as
//! wrapped functions that wrap their own arguments and results in turn.
//!
//! More information:
//!  - [ShadowRealm proposal][spec]
//!
//! [spec]: https://tc39.es/proposal-shadowrealm/

use std::{cell::RefCell, mem::MaybeUninit};

use boa_ast::operations::{ContainsSymbol, contains};
use boa_gc::{Finalize, Trace};
use boa_parser::{Parser, Source};
use dynify::Dynify;

use crate::{
    Context, JsArgs, JsData, JsError, JsExpect, JsNativeError, JsObject, JsResult, JsString,
    JsValue, NativeFunction,
    builtins::{
        BuiltInBuilder, BuiltInConstructor, BuiltInObject, IntrinsicObject, Promise,
        error::Error,
        eval::Eval,
        function::set_function_name,
        promise::{PromiseCapability, ResolvingFunctions},
    },
    context::{
        DefaultHooks,
        intrinsics::{Intrinsics, StandardConstructor, StandardConstructors},
    },
    job::NativeAsyncJob,
    js_string,
    module::{Module, ModuleRequest, Referrer},
    object::{
        FunctionObjectBuilder,
        builtins::JsPromise,
        internal_methods::{
            CallValue, InternalMethodCallContext, InternalObjectMethods, ORDINARY_INTERNAL_METHODS,
            get_prototype_from_constructor,
        },
    },
    property::{Attribute, PropertyDescriptor, PropertyKey},
    realm::Realm,
    string::StaticJsStrings,
    symbol::JsSymbol,
    value::IntegerOrInfinity,
};

/// The `ShadowRealm` constructor and prototype.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ShadowRealm;

/// The internal slots of a `ShadowRealm` instance.
#[derive(Debug, Trace, Finalize, JsData)]
struct ShadowRealmSlots {
    /// `[[ShadowRealm]]`: the realm the instance evaluates code in.
    realm: Realm,
}

impl IntrinsicObject for ShadowRealm {
    fn init(realm: &Realm) {
        BuiltInBuilder::from_standard_constructor::<Self>(realm)
            .method(Self::evaluate, js_string!("evaluate"), 1)
            .method(Self::import_value, js_string!("importValue"), 2)
            .property(
                JsSymbol::to_string_tag(),
                Self::NAME,
                Attribute::READONLY | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE,
            )
            .build();
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        Self::STANDARD_CONSTRUCTOR(intrinsics.constructors()).constructor()
    }
}

impl BuiltInObject for ShadowRealm {
    const NAME: JsString = js_string!("ShadowRealm");
}

impl BuiltInConstructor for ShadowRealm {
    const CONSTRUCTOR_ARGUMENTS: usize = 0;
    const PROTOTYPE_STORAGE_SLOTS: usize = 3;
    const CONSTRUCTOR_STORAGE_SLOTS: usize = 0;

    const STANDARD_CONSTRUCTOR: fn(&StandardConstructors) -> &StandardConstructor =
        StandardConstructors::shadow_realm;

    /// [`ShadowRealm ( )`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-shadowrealm/#sec-shadowrealm
    fn constructor(
        new_target: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. If NewTarget is undefined, throw a TypeError exception.
        if new_target.is_undefined() {
            return Err(JsNativeError::typ()
                .with_message("ShadowRealm: cannot call constructor without `new`")
                .into());
        }

        // 2. Let O be ? OrdinaryCreateFromConstructor(NewTarget, "%ShadowRealm.prototype%", « [[ShadowRealm]] »).
        let prototype = get_prototype_from_constructor(
            new_target,
            StandardConstructors::shadow_realm,
            context,
        )?;

        // 3. Let callerContext be the running execution context.
        // 4. Perform ? InitializeHostDefinedRealm().
        // 5. Let innerContext be the running execution context.
        // 6. Remove innerContext from the execution context stack and restore callerContext as the running execution context.
        // 7. Let realmRec be the Realm of innerContext.
        //
        // The global object of the new realm is always an ordinary object, whatever global object
        // the host hooks create for the realms of the host.
        let realm = Realm::create(&DefaultHooks, context.root_shape())?;
        let caller = context.enter_realm(realm.clone());
        let bindings = super::set_default_global_bindings(context);
        context.enter_realm(caller);
        bindings?;

        // 8. Set O.[[ShadowRealm]] to realmRec.
        // 9. Perform ? HostInitializeShadowRealm(realmRec, innerContext, O).
        // 10. Assert: realmRec.[[GlobalObject]] is an ordinary object.
        // 11. Return O.
        Ok(JsObject::from_proto_and_data_with_shared_shape(
            context.root_shape(),
            prototype,
            ShadowRealmSlots { realm },
        )
        .upcast()
        .into())
    }
}

impl ShadowRealm {
    /// [`ValidateShadowRealmObject ( O )`][spec]
    ///
    /// Returns the `[[ShadowRealm]]` of `O`.
    ///
    /// [spec]: https://tc39.es/proposal-shadowrealm/#sec-validateshadowrealmobject
    fn validate(this: &JsValue, method: &str) -> JsResult<Realm> {
        // 1. Perform ? RequireInternalSlot(O, [[ShadowRealm]]).
        this.as_object()
            .and_then(|object| {
                object
                    .downcast_ref::<ShadowRealmSlots>()
                    .map(|slots| slots.realm.clone())
            })
            .ok_or_else(|| {
                JsNativeError::typ()
                    .with_message(format!(
                        "ShadowRealm.prototype.{method}: `this` is not a ShadowRealm"
                    ))
                    .into()
            })
    }

    /// [`ShadowRealm.prototype.evaluate ( sourceText )`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-shadowrealm/#sec-shadowrealm.prototype.evaluate
    fn evaluate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be this value.
        // 2. Perform ? ValidateShadowRealmObject(O).
        let eval_realm = Self::validate(this, "evaluate")?;

        // 3. If sourceText is not a String, throw a TypeError exception.
        let Some(source_text) = args.get_or_undefined(0).as_string() else {
            return Err(JsNativeError::typ()
                .with_message("ShadowRealm.prototype.evaluate: the source text must be a string")
                .into());
        };

        // 4. Let callerRealm be the current Realm Record.
        let caller_realm = context.realm().clone();

        // 5. Let evalRealm be O.[[ShadowRealm]].
        // 6. Return ? PerformShadowRealmEval(sourceText, callerRealm, evalRealm).
        perform_shadow_realm_eval(&source_text, &caller_realm, eval_realm, context)
    }

    /// [`ShadowRealm.prototype.importValue ( specifier, exportName )`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-shadowrealm/#sec-shadowrealm.prototype.importvalue
    fn import_value(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be this value.
        // 2. Perform ? ValidateShadowRealmObject(O).
        let eval_realm = Self::validate(this, "importValue")?;

        // 3. Let specifierString be ? ToString(specifier).
        let specifier = args.get_or_undefined(0).to_string(context)?;

        // 4. If exportName is not a String, throw a TypeError exception.
        let Some(export_name) = args.get_or_undefined(1).as_string() else {
            return Err(JsNativeError::typ()
                .with_message("ShadowRealm.prototype.importValue: the export name must be a string")
                .into());
        };

        // 5. Let callerRealm be the current Realm Record.
        let caller_realm = context.realm().clone();

        // 6. Let evalRealm be O.[[ShadowRealm]].
        // 7. Return ShadowRealmImportValue(specifierString, exportName, callerRealm, evalRealm).
        shadow_realm_import_value(specifier, export_name, &caller_realm, eval_realm, context)
            .map(Into::into)
    }
}

/// Abstract operation [`PerformShadowRealmEval ( sourceText, callerRealm, evalRealm )`][spec].
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-performshadowrealmeval
fn perform_shadow_realm_eval(
    source_text: &JsString,
    caller_realm: &Realm,
    eval_realm: Realm,
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1. Perform ? HostEnsureCanCompileStrings(evalRealm, « », sourceText, false).
    context.host_hooks().ensure_can_compile_strings(
        eval_realm.clone(),
        &[],
        source_text,
        false,
        context,
    )?;

    // 2. Perform the following substeps in an implementation-defined order, possibly interleaving parsing and error detection:
    //     a. Let script be ParseText(StringToCodePoints(sourceText), Script).
    //     b. If script is a List of errors, throw a SyntaxError exception.
    //     c. If script Contains ScriptBody is false, return undefined.
    //     d. Let body be the ScriptBody of script.
    let source_text = source_text.to_vec();
    let mut parser = Parser::new(Source::from_utf16(&source_text));
    parser.set_identifier(context.next_parser_identifier());
    parser.set_native_stack_floor(context.vm.native_stack_floor);
    let (body, source) = parser.parse_eval(false, context.interner_mut())?;

    //     e. If body Contains NewTarget is true, throw a SyntaxError exception.
    //     f. If body Contains SuperProperty is true, throw a SyntaxError exception.
    //     g. If body Contains SuperCall is true, throw a SyntaxError exception.
    for (symbol, what) in [
        (ContainsSymbol::NewTarget, "`new.target` expression"),
        (ContainsSymbol::SuperProperty, "`super` reference"),
        (ContainsSymbol::SuperCall, "`super` call"),
    ] {
        if contains(&body, symbol) {
            return Err(JsNativeError::syntax()
                .with_message(format!("invalid {what} in ShadowRealm code"))
                .into());
        }
    }

    // 3. Let strictEval be ScriptIsStrict of script.
    let strict = body.strict();

    // 4. Let runningContext be the running execution context.
    // 5. If runningContext is not already suspended, suspend runningContext.
    // 6. Let evalContext be GetShadowRealmContext(evalRealm, strictEval).
    // 7. Let lexEnv be evalContext's LexicalEnvironment.
    // 8. Let varEnv be evalContext's VariableEnvironment.
    // 9. Push evalContext onto the execution context stack; evalContext is now the running execution context.
    let caller = context.enter_realm(eval_realm);

    // 10. Let result be Completion(EvalDeclarationInstantiation(body, varEnv, lexEnv, null, strictEval)).
    // 11. If result is a normal completion, then
    //     a. Set result to Completion(Evaluation of body).
    // 12. If result is a normal completion and result.[[Value]] is empty, then
    //     a. Set result to NormalCompletion(undefined).
    //
    // These are the steps of an indirect eval in the shadow realm.
    let result = Eval::evaluate_parsed(body, source, false, None, strict, context);

    // 13. Suspend evalContext and remove it from the execution context stack.
    // 14. Resume the context that is now on the top of the execution context stack as the running execution context.
    context.enter_realm(caller);

    // 15. If result is an abrupt completion, then
    //     a. Let copiedError be CreateTypeErrorCopy(callerRealm, result.[[Value]]).
    //     b. Return ThrowCompletion(copiedError).
    let value = result.map_err(|error| type_error_copy(error, "ShadowRealm code threw"))?;

    // 16. Return ? GetWrappedValue(callerRealm, result.[[Value]]).
    get_wrapped_value(caller_realm, &value, context)
}

/// Abstract operation [`ShadowRealmImportValue ( specifierString, exportNameString, callerRealm, evalRealm )`][spec].
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-shadowrealmimportvalue
fn shadow_realm_import_value(
    specifier: JsString,
    export_name: JsString,
    caller_realm: &Realm,
    eval_realm: Realm,
    context: &mut Context,
) -> JsResult<JsObject> {
    // 1. Let evalContext be GetShadowRealmContext(evalRealm, true).
    // 2. Let innerCapability be ! NewPromiseCapability(%Promise%).
    let (inner, resolvers) = JsPromise::new_pending(context);

    // 3. Let runningContext be the running execution context.
    // 4. If runningContext is not already suspended, suspend runningContext.
    // 5. Push evalContext onto the execution context stack; evalContext is now the running execution context.
    // 6. Let referrer be the Realm component of evalContext.
    // 7. Perform HostLoadImportedModule(referrer, specifierString, empty, innerCapability).
    // 8. Suspend evalContext and remove it from the execution context stack.
    // 9. Resume the context that is now on the top of the execution context stack as the running execution context.
    //
    // The module loads into the shadow realm, but a relative specifier resolves against the
    // script or module that called `importValue`, which a realm referrer has no path for.
    let base = context
        .get_active_script_or_module()
        .map_or_else(|| Referrer::Realm(eval_realm.clone()), Into::into);
    let request = ModuleRequest::new(specifier, Box::default());
    let realm = eval_realm.clone();
    let job = NativeAsyncJob::with_realm(
        async move |context| {
            import_module(base, request, realm, resolvers, context).await?;
            Ok(JsValue::undefined())
        },
        eval_realm,
    );
    context.enqueue_job(job.into());

    // 10. Let steps be the algorithm steps defined in ExportGetter functions.
    // 11. Let onFulfilled be CreateBuiltinFunction(steps, 1, "", « [[ExportNameString]] », callerRealm).
    // 12. Set onFulfilled.[[ExportNameString]] to exportNameString.
    let on_fulfilled = FunctionObjectBuilder::new(
        caller_realm,
        NativeFunction::from_copy_closure_with_captures(export_getter, export_name),
    )
    .name(js_string!())
    .length(1)
    .build();

    // 13. Let errorSteps be the algorithm steps defined in ImportValueError functions.
    // 14. Let onRejected be CreateBuiltinFunction(errorSteps, 1, "", « », callerRealm).
    let on_rejected =
        FunctionObjectBuilder::new(caller_realm, NativeFunction::from_fn_ptr(import_failed))
            .name(js_string!())
            .length(1)
            .build();

    // 15. Let promiseCapability be ! NewPromiseCapability(%Promise%).
    let capability = PromiseCapability::new(
        &caller_realm
            .intrinsics()
            .constructors()
            .promise()
            .constructor(),
        context,
    )
    .js_expect("operation cannot fail for the %Promise% intrinsic")?;
    let promise = capability.promise().clone();

    // 16. Return PerformPromiseThen(innerCapability.[[Promise]], onFulfilled, onRejected, promiseCapability).
    Promise::perform_promise_then(
        &inner,
        Some(on_fulfilled),
        Some(on_rejected),
        Some(capability),
        context,
    );
    Ok(promise)
}

/// The host part of `ShadowRealmImportValue`: loads the module `request` names into `realm`
/// through the module loader, links and evaluates it, and settles `resolvers` with its namespace
/// object or with the error that stopped it. This combines `HostLoadImportedModule`,
/// `FinishLoadingImportedModule` and `ContinueDynamicImport` for a realm referrer.
async fn import_module(
    base: Referrer,
    request: ModuleRequest,
    realm: Realm,
    resolvers: ResolvingFunctions,
    context: &RefCell<&mut Context>,
) -> JsResult<()> {
    let ResolvingFunctions { resolve, reject } = resolvers;
    let specifier = request.specifier().clone();
    let loader = context.borrow().module_loader();
    let fut = loader.load_imported_module(base, request, context);
    let mut stack = [MaybeUninit::<u8>::uninit(); 16];
    let mut heap = Vec::<MaybeUninit<u8>>::new();
    let completion = fut.init2(&mut stack, &mut heap).await;

    let context = &mut context.borrow_mut();
    let module = match completion {
        Ok(module) => module,
        Err(error) => {
            let error = error.into_opaque(context)?;
            reject.call(&JsValue::undefined(), &[error], context)?;
            return Ok(());
        }
    };

    realm
        .loaded_modules()
        .borrow_mut()
        .entry(specifier)
        .or_insert_with(|| module.clone());

    let evaluated = module.load_link_evaluate(context);
    let on_fulfilled = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, _, (module, resolve): &(Module, _), context| {
                let namespace = module.namespace(context);
                resolve.call(&JsValue::undefined(), &[namespace.into()], context)
            },
            (module, resolve),
        ),
    )
    .build();
    let on_rejected = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, args, reject, context| reject.call(&JsValue::undefined(), args, context),
            reject,
        ),
    )
    .build();
    Promise::perform_promise_then(
        &evaluated,
        Some(on_fulfilled),
        Some(on_rejected),
        None,
        context,
    );
    Ok(())
}

/// An [`ExportGetter`][spec] function, whose `[[ExportNameString]]` is `export_name`.
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-export-getter-functions
fn export_getter(
    _: &JsValue,
    args: &[JsValue],
    export_name: &JsString,
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1. Assert: exports is a module namespace exotic object.
    let exports = args
        .get_or_undefined(0)
        .as_object()
        .js_expect("an import resolves to a module namespace object")?;

    // 2. Let f be the active function object.
    // 3. Let string be f.[[ExportNameString]].
    // 4. Assert: Type(string) is String.
    // 5. Let hasOwn be ? HasOwnProperty(exports, string).
    // 6. If hasOwn is false, throw a TypeError exception.
    if !exports.has_own_property(export_name.clone(), context)? {
        return Err(JsNativeError::typ()
            .with_message(format!(
                "ShadowRealm.prototype.importValue: the module has no export named `{}`",
                export_name.to_std_string_escaped()
            ))
            .into());
    }

    // 7. Let value be ? Get(exports, string).
    let value = exports.get(export_name.clone(), context)?;

    // 8. Let realm be f.[[Realm]].
    // 9. Return ? GetWrappedValue(realm, value).
    let realm = context.realm().clone();
    get_wrapped_value(&realm, &value, context)
}

/// An [`ImportValueError`][spec] function: whatever stopped the import becomes a `TypeError` of
/// the caller realm.
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-import-value-error-functions
fn import_failed(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    // 1. Let realmRecord be the function's associated Realm Record.
    // 2. Let copiedError be CreateTypeErrorCopy(realmRecord, error).
    // 3. Return ThrowCompletion(copiedError).
    let error = JsError::from_opaque(args.get_or_undefined(0).clone());
    Err(type_error_copy(error, "ShadowRealm import failed"))
}

/// Abstract operation [`CreateTypeErrorCopy ( realmRecord, originalError )`][spec].
///
/// The copy is a native error, so it belongs to the realm it is thrown in, which is the caller
/// realm. Its message repeats the original one when that can be read without running code. An
/// error that script cannot catch, such as an exceeded runtime limit, passes through unchanged.
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-create-type-error-copy
fn type_error_copy(error: JsError, what: &str) -> JsError {
    if !error.is_catchable() {
        return error;
    }
    let message = match thrown_message(&error) {
        Some(original) => format!("{what}: {original}"),
        None => what.to_owned(),
    };
    JsNativeError::typ().with_message(message).into()
}

/// Describes a thrown value without calling into it: the kind and message of a native error,
/// the own `message` data property of an error object, or a primitive.
fn thrown_message(error: &JsError) -> Option<String> {
    if let Some(native) = error.as_native() {
        return Some(format!("{}: {}", native.kind(), native.message()));
    }
    let value = error.as_opaque()?;
    let Some(object) = value.as_object() else {
        return Some(value.display().to_string());
    };
    if !object.is::<Error>() {
        return None;
    }
    let message = object
        .borrow()
        .properties()
        .get(&PropertyKey::from(js_string!("message")))?;
    Some(message.value()?.as_string()?.to_std_string_escaped())
}

/// Abstract operation [`GetWrappedValue ( callerRealm, value )`][spec].
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-getwrappedvalue
fn get_wrapped_value(
    caller_realm: &Realm,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1. If value is an Object, then
    let Some(object) = value.as_object() else {
        // 2. Return value.
        return Ok(value.clone());
    };

    //     a. If IsCallable(value) is false, throw a TypeError exception.
    if !object.is_callable() {
        return Err(JsNativeError::typ()
            .with_message("ShadowRealm: only primitives and callables can cross realms")
            .into());
    }

    //     b. Return ? WrappedFunctionCreate(callerRealm, value).
    WrappedFunction::create(caller_realm, object, context).map(Into::into)
}

/// A [Wrapped Function Exotic Object][spec]: a function of the caller realm that calls a function
/// of another realm, wrapping the arguments and the result as they cross.
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-wrapped-function-exotic-objects
#[derive(Debug, Trace, Finalize)]
pub(crate) struct WrappedFunction {
    /// `[[WrappedTargetFunction]]`
    target: JsObject,
    /// `[[Realm]]`
    realm: Realm,
}

impl JsData for WrappedFunction {
    fn internal_methods(&self) -> &'static InternalObjectMethods {
        static METHODS: InternalObjectMethods = InternalObjectMethods {
            __call__: wrapped_function_call,
            ..ORDINARY_INTERNAL_METHODS
        };

        &METHODS
    }
}

impl WrappedFunction {
    /// Abstract operation [`WrappedFunctionCreate ( callerRealm, Target )`][spec].
    ///
    /// [spec]: https://tc39.es/proposal-shadowrealm/#sec-wrappedfunctioncreate
    fn create(caller_realm: &Realm, target: JsObject, context: &mut Context) -> JsResult<JsObject> {
        // 1. Let internalSlotsList be the internal slots listed in Table 2, plus [[Prototype]] and [[Extensible]].
        // 2. Let wrapped be MakeBasicObject(internalSlotsList).
        // 3. Set wrapped.[[Prototype]] to callerRealm.[[Intrinsics]].[[%Function.prototype%]].
        // 4. Set wrapped.[[Call]] as described in 2.1.
        // 5. Set wrapped.[[WrappedTargetFunction]] to Target.
        // 6. Set wrapped.[[Realm]] to callerRealm.
        let wrapped = JsObject::from_proto_and_data_with_shared_shape(
            context.root_shape(),
            caller_realm
                .intrinsics()
                .constructors()
                .function()
                .prototype(),
            Self {
                target: target.clone(),
                realm: caller_realm.clone(),
            },
        )
        .upcast();

        // 7. Let result be Completion(CopyNameAndLength(wrapped, Target)).
        // 8. If result is an abrupt completion, throw a TypeError exception.
        copy_name_and_length(&wrapped, &target, context).map_err(|error| {
            type_error_copy(
                error,
                "ShadowRealm: cannot copy the name and length of a function",
            )
        })?;

        // 9. Return wrapped.
        Ok(wrapped)
    }

    /// Gets the `[[Realm]]` of the wrapped function.
    pub(crate) const fn realm(&self) -> &Realm {
        &self.realm
    }
}

/// Abstract operation [`CopyNameAndLength ( F, Target [ , prefix [ , argCount ] ] )`][spec],
/// without a prefix and with an `argCount` of 0.
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-copynameandlength
fn copy_name_and_length(
    function: &JsObject,
    target: &JsObject,
    context: &mut Context,
) -> JsResult<()> {
    // 1. If argCount is not present, set argCount to 0.
    // 2. Let L be 0.
    let mut length = JsValue::new(0);

    // 3. Let targetHasLength be ? HasOwnProperty(Target, "length").
    // 4. If targetHasLength is true, then
    if target.has_own_property(StaticJsStrings::LENGTH, context)? {
        // a. Let targetLen be ? Get(Target, "length").
        let target_len = target.get(StaticJsStrings::LENGTH, context)?;
        // b. If targetLen is a Number, then
        if target_len.is_number() {
            match target_len
                .to_integer_or_infinity(context)
                .js_expect("to_integer_or_infinity cannot fail for a number")?
            {
                // i. If targetLen is +∞𝔽, set L to +∞.
                IntegerOrInfinity::PositiveInfinity => length = f64::INFINITY.into(),
                // ii. Else if targetLen is -∞𝔽, set L to 0.
                IntegerOrInfinity::NegativeInfinity => {}
                // iii. Else,
                IntegerOrInfinity::Integer(target_len) => {
                    // 1. Let targetLenAsInt be ! ToIntegerOrInfinity(targetLen).
                    // 2. Assert: targetLenAsInt is finite.
                    // 3. Set L to max(targetLenAsInt - argCount, 0).
                    length = target_len.max(0).into();
                }
            }
        }
    }

    // 5. Perform SetFunctionLength(F, L).
    function
        .define_property_or_throw(
            StaticJsStrings::LENGTH,
            PropertyDescriptor::builder()
                .value(length)
                .writable(false)
                .enumerable(false)
                .configurable(true),
            context,
        )
        .js_expect("defining the `length` property for a new object should not fail")?;

    // 6. Let targetName be ? Get(Target, "name").
    let target_name = target.get(js_string!("name"), context)?;

    // 7. If targetName is not a String, set targetName to the empty String.
    let target_name = target_name.as_string().unwrap_or_default();

    // 8. If prefix is present, then
    //     a. Perform SetFunctionName(F, targetName, prefix).
    // 9. Else,
    //     a. Perform SetFunctionName(F, targetName).
    set_function_name(function, &target_name.into(), None, context)
}

/// Internal method [`[[Call]] ( thisArgument, argumentsList )`][spec] of Wrapped Function Exotic
/// Objects.
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-wrapped-function-exotic-objects-call-thisargument-argumentslist
fn wrapped_function_call(
    obj: &JsObject,
    argument_count: usize,
    context: &mut InternalMethodCallContext<'_>,
) -> JsResult<CallValue> {
    let args = context
        .vm
        .stack
        .calling_convention_pop_arguments(argument_count);
    let _func = context.vm.stack.pop();
    let this = context.vm.stack.pop();

    context.check_runtime_limits()?;

    // 1. Let callerContext be the running execution context.
    // 2. Let calleeContext be PrepareForWrappedFunctionCall(F).
    // 3. Assert: calleeContext is now the running execution context.
    let (target, mut realm) = {
        let wrapped = obj.downcast_ref::<WrappedFunction>().js_expect(
            "wrapped function exotic method should only be callable from wrapped function objects",
        )?;
        (wrapped.target.clone(), wrapped.realm.clone())
    };
    context.swap_realm(&mut realm);

    // 4. Let result be Completion(OrdinaryWrappedFunctionCall(F, thisArgument, argumentsList)).
    let result = ordinary_wrapped_function_call(&target, &this, &args, context)
        .map_err(|error| error.inject_realm(context.realm().clone()));

    // 5. Remove calleeContext from the execution context stack and restore callerContext as the running execution context.
    context.swap_realm(&mut realm);

    // 6. Return ? result.
    context.vm.stack.push(result?);
    Ok(CallValue::Complete)
}

/// Abstract operation [`OrdinaryWrappedFunctionCall ( F, thisArgument, argumentsList )`][spec],
/// run in the realm of the wrapped function.
///
/// [spec]: https://tc39.es/proposal-shadowrealm/#sec-ordinary-wrapped-function-call
fn ordinary_wrapped_function_call(
    target: &JsObject,
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1. Let target be F.[[WrappedTargetFunction]].
    // 2. Assert: IsCallable(target) is true.
    // 3. Let callerRealm be F.[[Realm]].
    // 4. NOTE: Any exception objects produced after this point are associated with callerRealm.
    let caller_realm = context.realm().clone();

    // 5. Let targetRealm be ? GetFunctionRealm(target).
    let target_realm = target.get_function_realm(context)?;

    // 6. Let wrappedArgs be a new empty List.
    // 7. For each element arg of argumentsList, do
    //     a. Let wrappedValue be ? GetWrappedValue(targetRealm, arg).
    //     b. Append wrappedValue to wrappedArgs.
    let wrapped_args = args
        .iter()
        .map(|arg| get_wrapped_value(&target_realm, arg, context))
        .collect::<JsResult<Vec<_>>>()?;

    // 8. Let wrappedThisArgument be ? GetWrappedValue(targetRealm, thisArgument).
    let wrapped_this = get_wrapped_value(&target_realm, this, context)?;

    // 9. Let result be Completion(Call(target, wrappedThisArgument, wrappedArgs)).
    match target.call(&wrapped_this, &wrapped_args, context) {
        // 10. If result is a normal completion, then
        //     a. Return ? GetWrappedValue(callerRealm, result.[[Value]]).
        Ok(value) => get_wrapped_value(&caller_realm, &value, context),
        // 11. Else,
        //     a. Let copiedError be CreateTypeErrorCopy(callerRealm, result.[[Value]]).
        //     b. Return ThrowCompletion(copiedError).
        Err(error) => Err(type_error_copy(error, "wrapped function threw")),
    }
}
