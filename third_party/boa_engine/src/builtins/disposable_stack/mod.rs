//! Boa's implementation of explicit resource management: the `DisposableStack` and
//! `AsyncDisposableStack` objects and the dispose capabilities behind them and behind
//! `using` and `await using` declarations.
//!
//! More information:
//!  - [Proposal reference][spec]
//!
//! [spec]: https://tc39.es/proposal-explicit-resource-management/

use boa_gc::{Finalize, Gc, GcRefCell, Trace};

use crate::{
    Context, JsArgs, JsData, JsError, JsResult, JsString, JsValue,
    builtins::{
        BuiltInBuilder, BuiltInConstructor, BuiltInObject, IntrinsicObject,
        error::SuppressedError,
        promise::{Promise, PromiseCapability},
    },
    context::intrinsics::{Intrinsics, StandardConstructor, StandardConstructors},
    js_error, js_string,
    native_function::NativeFunction,
    object::{FunctionObjectBuilder, JsObject, internal_methods::get_prototype_from_constructor},
    property::Attribute,
    realm::Realm,
    symbol::JsSymbol,
};

/// The `[[Hint]]` of a `DisposableResource` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Trace, Finalize)]
#[boa_gc(empty_trace)]
pub(crate) enum DisposeHint {
    Sync,
    Async,
}

impl DisposeHint {
    pub(crate) const fn from_async(is_async: bool) -> Self {
        if is_async { Self::Async } else { Self::Sync }
    }
}

/// A `DisposableResource` record.
#[derive(Debug, Clone, Trace, Finalize)]
struct DisposableResource {
    value: JsValue,
    hint: DisposeHint,
    method: Option<JsObject>,
}

/// What the next step of an asynchronous `DisposeResources` asks of its caller.
pub(crate) enum DisposeStep {
    /// Every resource has been disposed.
    Done,
    /// Await `value`, then take the next step; a rejection is added to the completion.
    Await(JsValue),
}

/// A `DisposeCapability` record: the resources to dispose when a scope or stack ends.
#[derive(Debug, Default, Trace, Finalize, JsData)]
pub(crate) struct DisposeCapability {
    stack: Vec<DisposableResource>,
    needs_await: bool,
    has_awaited: bool,
}

impl DisposeCapability {
    /// Creates a new object holding an empty `DisposeCapability`.
    pub(crate) fn new_object() -> JsObject {
        JsObject::from_proto_and_data(None, Self::default())
    }

    /// `AddDisposableResource ( disposeCapability, V, hint [ , method ] )`
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-adddisposableresource
    pub(crate) fn add(
        &mut self,
        value: JsValue,
        hint: DisposeHint,
        method: Option<JsObject>,
        context: &mut Context,
    ) -> JsResult<()> {
        let resource = if let Some(method) = method {
            // 2. Else,
            //    a. Assert: V is undefined.
            //    b. Let resource be ? CreateDisposableResource(undefined, hint, method).
            if !method.is_callable() {
                return Err(js_error!(TypeError: "dispose method is not callable"));
            }
            DisposableResource {
                value: JsValue::undefined(),
                hint,
                method: Some(method),
            }
        } else {
            // 1. If method is not present, then
            //    a. If V is either null or undefined and hint is sync-dispose, return unused.
            if value.is_null_or_undefined() && hint == DisposeHint::Sync {
                return Ok(());
            }
            //    c. Let resource be ? CreateDisposableResource(V, hint).
            create_disposable_resource(value, hint, context)?
        };
        // 3. Append resource to disposeCapability.[[DisposableResourceStack]].
        self.stack.push(resource);
        // 4. Return unused.
        Ok(())
    }

    /// Runs `DisposeResources ( disposeCapability, completion )` up to its next `Await`.
    ///
    /// `error` is the completion: `Some` for a throw completion. A disposal that throws
    /// replaces it, wrapped in a `SuppressedError` when it was already a throw.
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-disposeresources
    pub(crate) fn step(
        capability: &JsObject,
        error: &mut Option<JsValue>,
        context: &mut Context,
    ) -> DisposeStep {
        loop {
            let resource = {
                let Some(mut cap) = capability.downcast_mut::<Self>() else {
                    return DisposeStep::Done;
                };
                // 3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
                let Some(resource) = cap.stack.pop() else {
                    // 4. If needsAwait is true and hasAwaited is false, then
                    if cap.needs_await && !cap.has_awaited {
                        // a. Perform ! Await(undefined).
                        cap.needs_await = false;
                        return DisposeStep::Await(JsValue::undefined());
                    }
                    // 6. Set disposeCapability.[[DisposableResourceStack]] to a new empty List.
                    return DisposeStep::Done;
                };
                // d. If hint is sync-dispose and needsAwait is true and hasAwaited is false, then
                if resource.hint == DisposeHint::Sync && cap.needs_await && !cap.has_awaited {
                    // i. Perform ! Await(undefined).
                    // ii. Set needsAwait to false.
                    cap.needs_await = false;
                    cap.stack.push(resource);
                    return DisposeStep::Await(JsValue::undefined());
                }
                resource
            };

            // e. If method is not undefined, then
            if let Some(method) = &resource.method {
                // i. Let result be Completion(Call(method, value)).
                match method.call(&resource.value, &[], context) {
                    // ii. If result is a normal completion and hint is async-dispose, then
                    Ok(result) if resource.hint == DisposeHint::Async => {
                        // 2. Set hasAwaited to true.
                        if let Some(mut cap) = capability.downcast_mut::<Self>() {
                            cap.has_awaited = true;
                        }
                        // 1. Set result to Completion(Await(result.[[Value]])).
                        return DisposeStep::Await(result);
                    }
                    Ok(_) => {}
                    // iii. If result is a throw completion, then
                    Err(err) => add_dispose_error(error, err, context),
                }
            } else {
                // f. Else,
                //    i. Assert: hint is async-dispose.
                //    ii. Set needsAwait to true.
                if let Some(mut cap) = capability.downcast_mut::<Self>() {
                    cap.needs_await = true;
                }
            }
        }
    }

    /// `DisposeResources ( disposeCapability, completion )` for a capability with only
    /// synchronous resources.
    pub(crate) fn dispose_sync(
        capability: &JsObject,
        error: &mut Option<JsValue>,
        context: &mut Context,
    ) {
        while let DisposeStep::Await(_) = Self::step(capability, error, context) {}
    }
}

/// Folds the throw completion `err` of a disposal into `error`, the completion so far.
pub(crate) fn add_dispose_error(error: &mut Option<JsValue>, err: JsError, context: &mut Context) {
    let err = err
        .into_opaque(context)
        .unwrap_or_else(|e| e.as_opaque().cloned().unwrap_or_else(JsValue::undefined));
    add_dispose_error_value(error, err, context);
}

/// Folds the thrown value `err` of a disposal into `error`, the completion so far.
pub(crate) fn add_dispose_error_value(
    error: &mut Option<JsValue>,
    err: JsValue,
    context: &mut Context,
) {
    // 1. If completion is a throw completion, then
    *error = Some(match error.take() {
        // a-f. Set completion to ThrowCompletion(a new SuppressedError of result and completion).
        Some(suppressed) => SuppressedError::create(err, suppressed, context),
        // 2. Else, set completion to result.
        None => err,
    });
}

/// `CreateDisposableResource ( V, hint [ , method ] )` without a `method`.
///
/// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-createdisposableresource
fn create_disposable_resource(
    value: JsValue,
    hint: DisposeHint,
    context: &mut Context,
) -> JsResult<DisposableResource> {
    // a. If V is either null or undefined, then
    if value.is_null_or_undefined() {
        // i. Set V to undefined.
        // ii. Set method to undefined.
        return Ok(DisposableResource {
            value: JsValue::undefined(),
            hint,
            method: None,
        });
    }
    // b. Else,
    //    i. If V is not an Object, throw a TypeError exception.
    if !value.is_object() {
        return Err(js_error!(TypeError: "using declaration value is not an object"));
    }
    //    ii. Set method to ? GetDisposeMethod(V, hint).
    //    iii. If method is undefined, throw a TypeError exception.
    let method = get_dispose_method(&value, hint, context)?
        .ok_or_else(|| js_error!(TypeError: "value has no Symbol.dispose method"))?;
    Ok(DisposableResource {
        value,
        hint,
        method: Some(method),
    })
}

/// `GetDisposeMethod ( V, hint )`
///
/// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-getdisposemethod
fn get_dispose_method(
    value: &JsValue,
    hint: DisposeHint,
    context: &mut Context,
) -> JsResult<Option<JsObject>> {
    // 1. If hint is async-dispose, then
    if hint == DisposeHint::Async {
        // a. Let method be ? GetMethod(V, @@asyncDispose).
        if let Some(method) = value.get_method(JsSymbol::async_dispose(), context)? {
            return Ok(Some(method));
        }
        // b. If method is undefined, then
        //    i. Set method to ? GetMethod(V, @@dispose).
        let Some(method) = value.get_method(JsSymbol::dispose(), context)? else {
            return Ok(None);
        };
        //    ii. If method is not undefined, then
        //        1. Let closure be a new Abstract Closure with no parameters that captures method ...
        //        3. Return CreateBuiltinFunction(closure, 0, "", « »).
        let closure = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_copy_closure_with_captures(
                |this, _, method: &JsObject, context| {
                    // b. Let promiseCapability be ! NewPromiseCapability(%Promise%).
                    let promise = context.intrinsics().constructors().promise().constructor();
                    let capability = PromiseCapability::new(&promise, context)?;
                    // c. Let result be Completion(Call(method, O)).
                    match method.call(this, &[], context) {
                        // d. IfAbruptRejectPromise(result, promiseCapability).
                        Err(err) => {
                            let err = err.into_opaque(context)?;
                            capability
                                .reject()
                                .call(&JsValue::undefined(), &[err], context)?;
                        }
                        // e. Perform ? Call(promiseCapability.[[Resolve]], undefined, « undefined »).
                        Ok(_) => {
                            capability.resolve().call(
                                &JsValue::undefined(),
                                &[JsValue::undefined()],
                                context,
                            )?;
                        }
                    }
                    // f. Return promiseCapability.[[Promise]].
                    Ok(capability.promise().clone().into())
                },
                method,
            ),
        )
        .name(js_string!())
        .length(0)
        .build();
        return Ok(Some(closure.into()));
    }
    // 2. Else,
    //    a. Let method be ? GetMethod(V, @@dispose).
    value.get_method(JsSymbol::dispose(), context)
}

/// The `DisposableStack` constructor.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DisposableStack;

/// The `AsyncDisposableStack` constructor.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AsyncDisposableStack;

/// The internal slots of a `DisposableStack` or `AsyncDisposableStack` instance.
#[derive(Debug, Trace, Finalize, JsData)]
pub(crate) struct DisposableStackData {
    /// Whether this is an `AsyncDisposableStack`.
    is_async: bool,
    /// The `[[DisposableState]]` or `[[AsyncDisposableState]]` slot: `true` once disposed.
    disposed: bool,
    /// The `[[DisposeCapability]]` slot.
    capability: JsObject,
}

const fn stack_name(is_async: bool) -> &'static str {
    if is_async {
        "AsyncDisposableStack"
    } else {
        "DisposableStack"
    }
}

/// `RequireInternalSlot(stack, [[DisposableState]])` (or `[[AsyncDisposableState]]`).
fn this_stack(
    this: &JsValue,
    is_async: bool,
    method: &str,
) -> JsResult<JsObject<DisposableStackData>> {
    this.as_object()
        .and_then(|o| o.downcast::<DisposableStackData>().ok())
        .filter(|o| o.borrow().data().is_async == is_async)
        .ok_or_else(|| {
            js_error!(
                TypeError: "{}.prototype.{} called on an incompatible receiver",
                stack_name(is_async),
                method
            )
        })
}

/// Throws the `ReferenceError` the methods of a disposed stack throw.
fn require_not_disposed(stack: &JsObject<DisposableStackData>, is_async: bool) -> JsResult<()> {
    if stack.borrow().data().disposed {
        return Err(js_error!(
            ReferenceError: "{} has already been disposed",
            stack_name(is_async)
        ));
    }
    Ok(())
}

fn new_stack(prototype: JsObject, is_async: bool, context: &Context) -> JsObject {
    // Set stack.[[DisposableState]] to pending.
    // Set stack.[[DisposeCapability]] to NewDisposeCapability().
    JsObject::from_proto_and_data_with_shared_shape(
        context.root_shape(),
        prototype,
        DisposableStackData {
            is_async,
            disposed: false,
            capability: DisposeCapability::new_object(),
        },
    )
    .upcast()
}

fn stack_constructor(
    new_target: &JsValue,
    is_async: bool,
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1. If NewTarget is undefined, throw a TypeError exception.
    if new_target.is_undefined() {
        return Err(js_error!(
            TypeError: "{} constructor requires 'new'",
            stack_name(is_async)
        ));
    }
    // 2. Let stack be ? OrdinaryCreateFromConstructor(NewTarget, "%DisposableStack.prototype%", ...).
    let prototype = get_prototype_from_constructor(
        new_target,
        if is_async {
            StandardConstructors::async_disposable_stack
        } else {
            StandardConstructors::disposable_stack
        },
        context,
    )?;
    // 5. Return stack.
    Ok(new_stack(prototype, is_async, context).into())
}

fn stack_init(realm: &Realm, is_async: bool) {
    let get_disposed = BuiltInBuilder::callable(
        realm,
        if is_async {
            get_disposed::<true>
        } else {
            get_disposed::<false>
        },
    )
    .name(js_string!("get disposed"))
    .build();

    let (dispose, dispose_name, dispose_symbol) = if is_async {
        (
            BuiltInBuilder::callable(realm, dispose_async)
                .name(js_string!("disposeAsync"))
                .length(0)
                .build(),
            js_string!("disposeAsync"),
            JsSymbol::async_dispose(),
        )
    } else {
        (
            BuiltInBuilder::callable(realm, dispose)
                .name(js_string!("dispose"))
                .length(0)
                .build(),
            js_string!("dispose"),
            JsSymbol::dispose(),
        )
    };

    let attribute = Attribute::WRITABLE | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE;
    let builder = if is_async {
        BuiltInBuilder::from_standard_constructor::<AsyncDisposableStack>(realm)
            .method(adopt::<true>, js_string!("adopt"), 2)
            .method(defer::<true>, js_string!("defer"), 1)
            .method(r#move::<true>, js_string!("move"), 0)
            .method(r#use::<true>, js_string!("use"), 1)
            .property(
                JsSymbol::to_string_tag(),
                AsyncDisposableStack::NAME,
                Attribute::READONLY | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE,
            )
    } else {
        BuiltInBuilder::from_standard_constructor::<DisposableStack>(realm)
            .method(adopt::<false>, js_string!("adopt"), 2)
            .method(defer::<false>, js_string!("defer"), 1)
            .method(r#move::<false>, js_string!("move"), 0)
            .method(r#use::<false>, js_string!("use"), 1)
            .property(
                JsSymbol::to_string_tag(),
                DisposableStack::NAME,
                Attribute::READONLY | Attribute::NON_ENUMERABLE | Attribute::CONFIGURABLE,
            )
    };
    builder
        .accessor(
            js_string!("disposed"),
            Some(get_disposed),
            None,
            Attribute::CONFIGURABLE | Attribute::NON_ENUMERABLE,
        )
        .property(dispose_name, dispose.clone(), attribute)
        .property(dispose_symbol, dispose, attribute)
        .build();
}

impl IntrinsicObject for DisposableStack {
    fn init(realm: &Realm) {
        stack_init(realm, false);
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        Self::STANDARD_CONSTRUCTOR(intrinsics.constructors()).constructor()
    }
}

impl BuiltInObject for DisposableStack {
    const NAME: JsString = js_string!("DisposableStack");
}

impl BuiltInConstructor for DisposableStack {
    const CONSTRUCTOR_ARGUMENTS: usize = 0;
    const PROTOTYPE_STORAGE_SLOTS: usize = 10;
    const CONSTRUCTOR_STORAGE_SLOTS: usize = 0;

    const STANDARD_CONSTRUCTOR: fn(&StandardConstructors) -> &StandardConstructor =
        StandardConstructors::disposable_stack;

    /// [`DisposableStack ( )`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-disposablestack
    fn constructor(
        new_target: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        stack_constructor(new_target, false, context)
    }
}

impl IntrinsicObject for AsyncDisposableStack {
    fn init(realm: &Realm) {
        stack_init(realm, true);
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        Self::STANDARD_CONSTRUCTOR(intrinsics.constructors()).constructor()
    }
}

impl BuiltInObject for AsyncDisposableStack {
    const NAME: JsString = js_string!("AsyncDisposableStack");
}

impl BuiltInConstructor for AsyncDisposableStack {
    const CONSTRUCTOR_ARGUMENTS: usize = 0;
    const PROTOTYPE_STORAGE_SLOTS: usize = 10;
    const CONSTRUCTOR_STORAGE_SLOTS: usize = 0;

    const STANDARD_CONSTRUCTOR: fn(&StandardConstructors) -> &StandardConstructor =
        StandardConstructors::async_disposable_stack;

    /// [`AsyncDisposableStack ( )`][spec]
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-asyncdisposablestack
    fn constructor(
        new_target: &JsValue,
        _: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        stack_constructor(new_target, true, context)
    }
}

/// `get DisposableStack.prototype.disposed` and `get AsyncDisposableStack.prototype.disposed`
fn get_disposed<const ASYNC: bool>(
    this: &JsValue,
    _: &[JsValue],
    _: &mut Context,
) -> JsResult<JsValue> {
    // 1-2. Perform ? RequireInternalSlot(disposableStack, [[DisposableState]]).
    let stack = this_stack(this, ASYNC, "disposed")?;
    // 3. If disposableStack.[[DisposableState]] is disposed, return true.
    // 4. Otherwise, return false.
    Ok(stack.borrow().data().disposed.into())
}

/// `DisposableStack.prototype.use ( value )` and `AsyncDisposableStack.prototype.use ( value )`
fn r#use<const ASYNC: bool>(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1-2. Perform ? RequireInternalSlot(disposableStack, [[DisposableState]]).
    let stack = this_stack(this, ASYNC, "use")?;
    // 3. If disposableStack.[[DisposableState]] is disposed, throw a ReferenceError exception.
    require_not_disposed(&stack, ASYNC)?;
    // 4. Perform ? AddDisposableResource(disposableStack.[[DisposeCapability]], value, sync-dispose).
    let value = args.get_or_undefined(0).clone();
    let capability = stack.borrow().data().capability.clone();
    let mut cap = capability
        .downcast_mut::<DisposeCapability>()
        .ok_or_else(|| js_error!(TypeError: "invalid dispose capability"))?;
    cap.add(value.clone(), DisposeHint::from_async(ASYNC), None, context)?;
    // 5. Return value.
    Ok(value)
}

/// `DisposableStack.prototype.adopt ( value, onDispose )` and its asynchronous counterpart.
fn adopt<const ASYNC: bool>(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1-2. Perform ? RequireInternalSlot(disposableStack, [[DisposableState]]).
    let stack = this_stack(this, ASYNC, "adopt")?;
    // 3. If disposableStack.[[DisposableState]] is disposed, throw a ReferenceError exception.
    require_not_disposed(&stack, ASYNC)?;
    // 4. If IsCallable(onDispose) is false, throw a TypeError exception.
    let value = args.get_or_undefined(0).clone();
    let on_dispose = args
        .get_or_undefined(1)
        .as_callable()
        .ok_or_else(|| js_error!(TypeError: "onDispose is not callable"))?;
    // 5. Let closure be a new Abstract Closure with no parameters that captures value and onDispose ...
    //    a. Return ? Call(onDispose, undefined, « value »).
    // 6. Let F be CreateBuiltinFunction(closure, 0, "", « »).
    let f = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, _, (on_dispose, value): &(JsObject, JsValue), context| {
                on_dispose.call(&JsValue::undefined(), std::slice::from_ref(value), context)
            },
            (on_dispose, value.clone()),
        ),
    )
    .name(js_string!())
    .length(0)
    .build();
    // 7. Perform ? AddDisposableResource(disposableStack.[[DisposeCapability]], undefined, sync-dispose, F).
    let capability = stack.borrow().data().capability.clone();
    let mut cap = capability
        .downcast_mut::<DisposeCapability>()
        .ok_or_else(|| js_error!(TypeError: "invalid dispose capability"))?;
    cap.add(
        JsValue::undefined(),
        DisposeHint::from_async(ASYNC),
        Some(f.into()),
        context,
    )?;
    // 8. Return value.
    Ok(value)
}

/// `DisposableStack.prototype.defer ( onDispose )` and its asynchronous counterpart.
fn defer<const ASYNC: bool>(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1-2. Perform ? RequireInternalSlot(disposableStack, [[DisposableState]]).
    let stack = this_stack(this, ASYNC, "defer")?;
    // 3. If disposableStack.[[DisposableState]] is disposed, throw a ReferenceError exception.
    require_not_disposed(&stack, ASYNC)?;
    // 4. If IsCallable(onDispose) is false, throw a TypeError exception.
    let on_dispose = args
        .get_or_undefined(0)
        .as_callable()
        .ok_or_else(|| js_error!(TypeError: "onDispose is not callable"))?;
    // 5. Perform ? AddDisposableResource(disposableStack.[[DisposeCapability]], undefined, sync-dispose, onDispose).
    let capability = stack.borrow().data().capability.clone();
    let mut cap = capability
        .downcast_mut::<DisposeCapability>()
        .ok_or_else(|| js_error!(TypeError: "invalid dispose capability"))?;
    cap.add(
        JsValue::undefined(),
        DisposeHint::from_async(ASYNC),
        Some(on_dispose),
        context,
    )?;
    // 6. Return undefined.
    Ok(JsValue::undefined())
}

/// `DisposableStack.prototype.move ( )` and `AsyncDisposableStack.prototype.move ( )`
fn r#move<const ASYNC: bool>(
    this: &JsValue,
    _: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1-2. Perform ? RequireInternalSlot(disposableStack, [[DisposableState]]).
    let stack = this_stack(this, ASYNC, "move")?;
    // 3. If disposableStack.[[DisposableState]] is disposed, throw a ReferenceError exception.
    require_not_disposed(&stack, ASYNC)?;
    // 4. Let newDisposableStack be ? OrdinaryCreateFromConstructor(%DisposableStack%, "%DisposableStack.prototype%", ...).
    let constructors = context.intrinsics().constructors();
    let prototype = if ASYNC {
        constructors.async_disposable_stack().prototype()
    } else {
        constructors.disposable_stack().prototype()
    };
    let new_stack = new_stack(prototype, ASYNC, context);
    // 6. Set newDisposableStack.[[DisposeCapability]] to disposableStack.[[DisposeCapability]].
    // 7. Set disposableStack.[[DisposeCapability]] to NewDisposeCapability().
    // 8. Set disposableStack.[[DisposableState]] to disposed.
    let fresh = DisposeCapability::new_object();
    let moved = {
        let mut stack = stack.borrow_mut();
        let data = stack.data_mut();
        data.disposed = true;
        std::mem::replace(&mut data.capability, fresh)
    };
    if let Some(mut data) = new_stack.downcast_mut::<DisposableStackData>() {
        data.capability = moved;
    }
    // 9. Return newDisposableStack.
    Ok(new_stack.into())
}

/// [`DisposableStack.prototype.dispose ( )`][spec]
///
/// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-disposablestack.prototype.dispose
fn dispose(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    // 1-2. Perform ? RequireInternalSlot(disposableStack, [[DisposableState]]).
    let stack = this_stack(this, false, "dispose")?;
    // 3. If disposableStack.[[DisposableState]] is disposed, return undefined.
    // 4. Set disposableStack.[[DisposableState]] to disposed.
    let capability = {
        let mut stack = stack.borrow_mut();
        let data = stack.data_mut();
        if data.disposed {
            return Ok(JsValue::undefined());
        }
        data.disposed = true;
        data.capability.clone()
    };
    // 5. Return ? DisposeResources(disposableStack.[[DisposeCapability]], NormalCompletion(undefined)).
    let mut error = None;
    DisposeCapability::dispose_sync(&capability, &mut error, context);
    match error {
        Some(error) => Err(JsError::from_opaque(error)),
        None => Ok(JsValue::undefined()),
    }
}

/// The state of an `AsyncDisposableStack.prototype.disposeAsync` in progress.
#[derive(Trace, Finalize)]
struct AsyncDispose {
    capability: JsObject,
    error: Option<JsValue>,
    result: PromiseCapability,
}

/// Runs the asynchronous `DisposeResources` of `state` until its next `Await` or its end.
fn continue_async_dispose(state: &Gc<GcRefCell<AsyncDispose>>, context: &mut Context) {
    loop {
        let capability = state.borrow().capability.clone();
        let mut error = state.borrow_mut().error.take();
        let step = DisposeCapability::step(&capability, &mut error, context);
        state.borrow_mut().error = error;
        match step {
            DisposeStep::Done => {
                let state = state.borrow();
                let (function, value) = match &state.error {
                    // 7. IfAbruptRejectPromise(result, promiseCapability).
                    Some(error) => (state.result.reject().clone(), error.clone()),
                    // 8. Perform ! Call(promiseCapability.[[Resolve]], undefined, « result »).
                    None => (state.result.resolve().clone(), JsValue::undefined()),
                };
                drop(state);
                drop(function.call(&JsValue::undefined(), &[value], context));
                return;
            }
            DisposeStep::Await(value) => {
                // Await ( value ): Let promise be ? PromiseResolve(%Promise%, value).
                let constructor = context.intrinsics().constructors().promise().constructor();
                let promise = match Promise::promise_resolve(&constructor, value, context) {
                    Ok(promise) => promise,
                    Err(err) => {
                        let mut error = state.borrow_mut().error.take();
                        add_dispose_error(&mut error, err, context);
                        state.borrow_mut().error = error;
                        continue;
                    }
                };
                let Ok(promise) = promise.downcast::<Promise>() else {
                    continue;
                };
                let on_fulfilled = FunctionObjectBuilder::new(
                    context.realm(),
                    NativeFunction::from_copy_closure_with_captures(
                        |_, _, state: &Gc<GcRefCell<AsyncDispose>>, context| {
                            continue_async_dispose(state, context);
                            Ok(JsValue::undefined())
                        },
                        state.clone(),
                    ),
                )
                .length(1)
                .build();
                let on_rejected = FunctionObjectBuilder::new(
                    context.realm(),
                    NativeFunction::from_copy_closure_with_captures(
                        |_, args, state: &Gc<GcRefCell<AsyncDispose>>, context| {
                            let mut error = state.borrow_mut().error.take();
                            add_dispose_error_value(
                                &mut error,
                                args.get_or_undefined(0).clone(),
                                context,
                            );
                            state.borrow_mut().error = error;
                            continue_async_dispose(state, context);
                            Ok(JsValue::undefined())
                        },
                        state.clone(),
                    ),
                )
                .length(1)
                .build();
                Promise::perform_promise_then(
                    &promise,
                    Some(on_fulfilled),
                    Some(on_rejected),
                    None,
                    context,
                );
                return;
            }
        }
    }
}

/// [`AsyncDisposableStack.prototype.disposeAsync ( )`][spec]
///
/// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-asyncdisposablestack.prototype.disposeAsync
fn dispose_async(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    // 2. Let promiseCapability be ! NewPromiseCapability(%Promise%).
    let constructor = context.intrinsics().constructors().promise().constructor();
    let result = PromiseCapability::new(&constructor, context)?;
    let promise = result.promise().clone();

    // 3. If asyncDisposableStack does not have an [[AsyncDisposableState]] internal slot, then
    let stack = match this_stack(this, true, "disposeAsync") {
        Ok(stack) => stack,
        Err(err) => {
            // a. Perform ! Call(promiseCapability.[[Reject]], undefined, « a newly created TypeError object »).
            let err = err.into_opaque(context)?;
            result
                .reject()
                .call(&JsValue::undefined(), &[err], context)?;
            // b. Return promiseCapability.[[Promise]].
            return Ok(promise.into());
        }
    };

    // 4. If asyncDisposableStack.[[AsyncDisposableState]] is disposed, then
    // 5. Set asyncDisposableStack.[[AsyncDisposableState]] to disposed.
    let capability = {
        let mut stack = stack.borrow_mut();
        let data = stack.data_mut();
        if data.disposed {
            drop(stack);
            // a. Perform ! Call(promiseCapability.[[Resolve]], undefined, « undefined »).
            result
                .resolve()
                .call(&JsValue::undefined(), &[JsValue::undefined()], context)?;
            // b. Return promiseCapability.[[Promise]].
            return Ok(promise.into());
        }
        data.disposed = true;
        data.capability.clone()
    };

    // 6. Let result be Completion(DisposeResources(asyncDisposableStack.[[DisposeCapability]], NormalCompletion(undefined))).
    let state = Gc::new(GcRefCell::new(AsyncDispose {
        capability,
        error: None,
        result,
    }));
    continue_async_dispose(&state, context);

    // 9. Return promiseCapability.[[Promise]].
    Ok(promise.into())
}

/// [`%AsyncIteratorPrototype% [ %Symbol.asyncDispose% ] ( )`][spec]
///
/// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-%asynciteratorprototype%-@@asyncdispose
pub(crate) fn async_iterator_dispose(
    this: &JsValue,
    _: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1. Let O be the this value.
    // 2. Let promiseCapability be ! NewPromiseCapability(%Promise%).
    let constructor = context.intrinsics().constructors().promise().constructor();
    let capability = PromiseCapability::new(&constructor, context)?;

    let reject = |err: JsError, capability: &PromiseCapability, context: &mut Context| {
        let err = err.into_opaque(context)?;
        capability
            .reject()
            .call(&JsValue::undefined(), &[err], context)?;
        Ok::<JsValue, JsError>(capability.promise().clone().into())
    };

    // 3. Let return be Completion(GetMethod(O, "return")).
    // 4. IfAbruptRejectPromise(return, promiseCapability).
    let r#return = match this.get_method(js_string!("return"), context) {
        Ok(r#return) => r#return,
        Err(err) => return reject(err, &capability, context),
    };

    let Some(r#return) = r#return else {
        // 5. If return is undefined, then
        //    a. Perform ! Call(promiseCapability.[[Resolve]], undefined, « undefined »).
        capability
            .resolve()
            .call(&JsValue::undefined(), &[JsValue::undefined()], context)?;
        return Ok(capability.promise().clone().into());
    };

    // 6. Else,
    //    a. Let result be Completion(Call(return, O, « »)).
    //    b. IfAbruptRejectPromise(result, promiseCapability).
    let result = match r#return.call(this, &[], context) {
        Ok(result) => result,
        Err(err) => return reject(err, &capability, context),
    };

    //    c. Let resultWrapper be Completion(PromiseResolve(%Promise%, result)).
    //    d. IfAbruptRejectPromise(resultWrapper, promiseCapability).
    let wrapper = match Promise::promise_resolve(&constructor, result, context) {
        Ok(wrapper) => wrapper,
        Err(err) => return reject(err, &capability, context),
    };
    let Ok(wrapper) = wrapper.downcast::<Promise>() else {
        return Ok(capability.promise().clone().into());
    };

    //    e. Let unwrap be a new Abstract Closure that performs the following steps when called:
    //       i. Return undefined.
    //    f. Let onFulfilled be CreateBuiltinFunction(unwrap, 1, "", « »).
    let on_fulfilled = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_fn_ptr(|_, _, _| Ok(JsValue::undefined())),
    )
    .name(js_string!())
    .length(1)
    .build();

    //    g. Perform PerformPromiseThen(resultWrapper, onFulfilled, undefined, promiseCapability).
    let promise = capability.promise().clone();
    Promise::perform_promise_then(
        &wrapper,
        Some(on_fulfilled),
        None,
        Some(capability),
        context,
    );

    // 7. Return promiseCapability.[[Promise]].
    Ok(promise.into())
}
